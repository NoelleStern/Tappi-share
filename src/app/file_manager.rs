use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::atomic,
    time::SystemTime,
};
use walkdir::WalkDir;

pub type FileId = usize;
static NEXT_OUTPUT_FILEID: atomic::AtomicUsize = atomic::AtomicUsize::new(0);
fn get_new_output_file_id() -> usize {
    NEXT_OUTPUT_FILEID.fetch_add(1, atomic::Ordering::Relaxed) // Get and increment
}

pub struct FileManager {
    pub ignore_empty: bool,                 // Should it ignore empty directories
    pub output_queue: VecDeque<OutputFile>, // Regulates the queue
    pub input_map: IndexMap<FileId, InputFile>, // Input file list
    pub output_map: IndexMap<FileId, OutputFile>, // Output file list
}
impl FileManager {
    pub fn new(ignore_empty: bool) -> Self {
        Self {
            ignore_empty,
            output_queue: VecDeque::default(),
            input_map: IndexMap::default(),
            output_map: IndexMap::default(),
        }
    }
}
impl FileManager {
    pub fn add_output_files(&mut self, files: &Vec<PathBuf>) -> color_eyre::Result<()> {
        let mut output_files: Vec<OutputFile> = vec![];

        // Walk directory recursively if path is a directory
        for path in files {
            if path.is_dir() {
                // Contains empty directories to preserve the structure
                let mut empty_directories: Vec<PathBuf> = vec![];
                if !self.ignore_empty {
                    for entry in WalkDir::new(path)
                        .into_iter()
                        .filter_map(Result::ok)
                        .filter(|e| e.file_type().is_dir())
                    {
                        // Check if directory is empty
                        let path = entry.path();
                        if fs::read_dir(path)
                            .map(|mut i| i.next().is_none())
                            .unwrap_or(false)
                        {
                            empty_directories.push(path.into());
                        }
                    }
                }

                // Contains all files
                let directory_files: Vec<PathBuf> = WalkDir::new(path)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_file())
                    .map(|entry| entry.path().to_path_buf())
                    .collect();

                // Add output files to the list
                for p in empty_directories {
                    let of = OutputFile::new(p, Some(path.clone()), true)?;
                    output_files.push(of);
                }
                for p in directory_files {
                    let of = OutputFile::new(p, Some(path.clone()), false)?;
                    output_files.push(of);
                }
            } else {
                let of = OutputFile::new(path.clone(), None, false)?;
                output_files.push(of);
            }
        }

        self.output_queue.extend(output_files.iter().cloned());

        for file in output_files {
            self.output_map.insert(file.id, file.clone());
        }

        Ok(())
    }

    // fn add_input_files(&mut self, files: Vec<InputFile>) {
    //     self.input_files.push(file);
    // }

    pub fn get_next_output_file(&mut self) -> Option<OutputFile> {
        self.output_queue.pop_front()
    }
    pub fn get_input_map(&self) -> IndexMap<&FileId, &InputFile> {
        self.input_map.iter().collect()
    }
    pub fn get_output_map_no_dir(&self) -> IndexMap<&FileId, &OutputFile> {
        self.output_map
            .iter()
            .filter(|v| !v.1.meta.is_dir)
            .collect()
    }

    pub fn set_output_finished(&mut self, id: FileId) {
        if let Some(output_file) = self.output_map.get_mut(&id) {
            output_file.finished = true;
        }
    }
    pub fn add_input_report(&mut self, report: SpeedReport) {
        if let Some(output_file) = self.input_map.get_mut(&report.file_id) {
            output_file.speed_counter.add_report(report);
        }
    }
    pub fn add_output_report(&mut self, report: SpeedReport) {
        if let Some(output_file) = self.output_map.get_mut(&report.file_id) {
            output_file.speed_counter.add_report(report);
        }
    }
    // in seconds
    pub fn get_estimate<P: ProgressFile>(files: &IndexMap<FileId, P>) -> f64 {
        let mut total_size: f64 = 0.0;
        for (_i, f) in files {
            if !f.get_meta().is_dir && !f.get_finished() {
                total_size += (f.get_meta().size as f64) * (1.0 - f.get_progress());
            }
        }

        if total_size > 0.0 {
            let speed = Self::get_average_speed(files);
            (total_size * 8.0 / 1_000_000.0) / speed
        } else {
            0.0
        }
    }
    pub fn get_average_speed<P: ProgressFile>(files: &IndexMap<FileId, P>) -> f64 {
        let mut speed: f64 = 0.0;
        let mut counter: usize = 0;

        for (_i, f) in files {
            let s = f.get_speed();
            if s > 0.0 {
                speed += s;
                counter += 1;
            }
        }

        if counter > 0 && speed > 0.0 {
            speed / (counter as f64)
        } else {
            0.0
        }
    }
    pub fn get_completion<P: ProgressFile>(files: &IndexMap<FileId, P>) -> bool {
        if !files.is_empty() {
            let mut result = true;
            for (_i, f) in files {
                if !f.get_finished() {
                    result = false;
                }
            }
            result
        } else {
            false
        }
    }
}

pub trait ProgressFile {
    fn get_name(&self) -> Option<&str>;
    fn get_progress(&self) -> f64;
    fn get_finished(&self) -> bool;
    fn get_speed(&self) -> f64;
    fn get_meta(&self) -> &MetaData;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputFile {
    pub id: FileId,
    pub meta: MetaData,
    pub progress: f64,
    pub finished: bool,
    pub speed_counter: SpeedCounter,
}
impl OutputFile {
    fn new(path: PathBuf, base_path: Option<PathBuf>, is_dir: bool) -> color_eyre::Result<Self> {
        let meta: MetaData = if is_dir {
            MetaData::new(&path, 0, base_path.clone(), true)
        } else {
            let metadata = fs::metadata(path.clone())?;
            MetaData::new(&path, metadata.len() as usize, base_path.clone(), false)
        };

        Ok(Self {
            id: get_new_output_file_id(),
            meta,
            progress: 0.0,
            finished: false,
            speed_counter: SpeedCounter::default(),
        })
    }
}
impl ProgressFile for OutputFile {
    fn get_name(&self) -> Option<&str> {
        let name = self.meta.path.file_name();
        if let Some(name) = name {
            name.to_str()
        } else {
            None
        }
    }
    fn get_progress(&self) -> f64 {
        self.progress
    }
    fn get_finished(&self) -> bool {
        self.finished
    }
    fn get_speed(&self) -> f64 {
        self.speed_counter.get_speed().unwrap_or(0.0)
    }
    fn get_meta(&self) -> &MetaData {
        &self.meta
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputFile {
    pub id: FileId,
    pub meta: MetaData,
    pub progress: f64,
    pub speed_counter: SpeedCounter,
}
impl InputFile {
    pub fn new(id: usize, meta: MetaData) -> Self {
        Self {
            id,
            meta,
            progress: 0.0,
            speed_counter: SpeedCounter::default(),
        }
    }
}
impl ProgressFile for InputFile {
    fn get_name(&self) -> Option<&str> {
        Some(&self.meta.name)
    }
    fn get_progress(&self) -> f64 {
        self.progress
    }
    fn get_finished(&self) -> bool {
        self.progress >= 1.0
    }
    fn get_speed(&self) -> f64 {
        self.speed_counter.get_speed().unwrap_or(0.0)
    }
    fn get_meta(&self) -> &MetaData {
        &self.meta
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetaData {
    pub is_dir: bool,
    pub path: PathBuf,
    pub base_path: Option<PathBuf>,
    pub name: String,
    pub extension: String,
    pub size: usize,
    pub progress_bytes: usize,
}
impl MetaData {
    pub fn new(path: &Path, size: usize, base_path: Option<PathBuf>, is_dir: bool) -> Self {
        let p = MetaData::normalize_path(path);
        let mut name: String = "".to_string();
        let mut extension: String = "".to_string();

        if let Some(n) = p.file_name()
            && let Some(s) = n.to_str()
        {
            name = s.into();
            extension = s.into();
        };

        Self {
            is_dir,
            base_path,
            name,
            extension,
            size,
            progress_bytes: 0,
            path: p,
        }
    }
    fn normalize_path(path: &Path) -> PathBuf {
        path.to_string_lossy().replace('\\', "/").into()
    }
    // TODO: find a less stupid way to do this, please?
    pub fn local_path(&self) -> Option<PathBuf> {
        let mut result: Option<PathBuf> = None;

        if let Some(path) = self.base_path.clone() {
            let cloned_path = path.clone();
            let parent = cloned_path.file_name();
            if let Some(parent) = parent {
                let stripped_path = self.path.strip_prefix(path);
                if let Ok(stripped_path) = stripped_path {
                    let mut os_string = parent.to_os_string();
                    os_string.push("/");
                    os_string.push(stripped_path);
                    result = Some(os_string.into());
                }
            }
        }

        result
    }
    pub fn get_path(&self) -> PathBuf {
        if let Some(local_path) = self.local_path() {
            local_path
        } else {
            self.name.clone().into()
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileProgressReport {
    pub file_id: FileId,
    pub progress: f64,
}
impl FileProgressReport {
    pub fn new(file_id: FileId, progress: f64) -> Self {
        Self { file_id, progress }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpeedReport {
    file_id: FileId,
    timestamp: SystemTime,
    bytes: usize,
}
impl SpeedReport {
    pub fn new(file_id: FileId, bytes: usize) -> Self {
        Self {
            file_id,
            bytes,
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpeedCounter {
    report_buffer: VecDeque<SpeedReport>,
}
impl Default for SpeedCounter {
    fn default() -> Self {
        Self {
            report_buffer: VecDeque::with_capacity(SpeedCounter::CAPACITY),
        }
    }
}
impl SpeedCounter {
    const CAPACITY: usize = 10;

    fn add_report(&mut self, report: SpeedReport) {
        if self.report_buffer.len() == SpeedCounter::CAPACITY {
            self.report_buffer.pop_front();
        }
        self.report_buffer.push_back(report);
    }
    fn get_speed(&self) -> Option<f64> {
        if self.report_buffer.len() > 1 {
            let beginning = self.report_buffer[0].timestamp;
            let end = self.report_buffer[self.report_buffer.len() - 1].timestamp;
            let duration = end.duration_since(beginning).unwrap(); // Should be fine since the messages are ordered

            let mut byte_sum: f64 = 0.0;
            for i in 1..self.report_buffer.len() {
                byte_sum += self.report_buffer[i].bytes as f64;
            }

            let megabits = (byte_sum * 8.0) / 1_000_000.0;
            let speed = megabits / duration.as_secs_f64(); // Mbps
            Some(speed)
        } else {
            None
        }
    }
}
