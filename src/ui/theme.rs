use config::{Config, File, FileFormat};
use ratatui::style::Color;
use serde::Deserialize;

static DEFAULT_THEME: &str = include_str!("../config/themes/catpuccin_frappe.toml");

#[derive(Deserialize, Clone)]
pub struct Theme {
    pub surface0: ThemeColor,
    pub surface1: ThemeColor,
    pub surface2: ThemeColor,
    pub primary: ThemeColor,
    pub accent: ThemeColor,
    pub text: ThemeColor,
    pub info: ThemeColor,
    pub success: ThemeColor,
    pub error: ThemeColor,
    pub warning: ThemeColor,
}
impl Theme {
    pub fn load_default() -> color_eyre::Result<Theme> {
        let default_source = File::from_str(DEFAULT_THEME, FileFormat::Toml);
        let cfg = Config::builder().add_source(default_source).build()?;
        Ok(cfg.try_deserialize()?)
    }
}

pub struct ThemeColor(Color);
impl Clone for ThemeColor {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}
impl From<ThemeColor> for Color {
    fn from(color: ThemeColor) -> Self {
        color.0
    }
}
impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim_start_matches('#');

        if trimmed.len() != 6 {
            return Err(serde::de::Error::custom("Non-matching length"));
        }

        let rgb = u32::from_str_radix(trimmed, 16).map_err(serde::de::Error::custom)?;
        let r = ((rgb >> 16) & 0xFF) as u8;
        let g = ((rgb >> 8) & 0xFF) as u8;
        let b = (rgb & 0xFF) as u8;

        Ok(ThemeColor(Color::Rgb(r, g, b)))
    }
}
