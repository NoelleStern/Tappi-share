use crate::{
    app::{app_event::AppEventClient, event::BasicEventSenderExt, models::Maid},
    cli::ClientArgs,
    client::{
        rtc_base::WebConnection,
        signaling::{negotiator::negotiate, signaling_manual::SignalingManual},
    },
};

pub async fn init(
    maid: Maid,
    signaling_manual: Option<SignalingManual>,
    args: ClientArgs,
) -> color_eyre::Result<()> {
    // Init WebRTC connection
    let wc = WebConnection::new(maid.clone(), &args).await?;
    let pc = wc.pc.clone();
    maid.event_tx
        .send_event(AppEventClient::InitConnection(wc))
        .await;

    // Negotiate
    negotiate(pc, args, maid, signaling_manual).await?;

    Ok(())
}
