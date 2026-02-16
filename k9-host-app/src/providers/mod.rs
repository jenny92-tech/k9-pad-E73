// INPUT:  tokio, k9-datachannel-proto
// OUTPUT: Provider trait + time/volume/bilibili providers
// POS:    数据提供者模块，从 k9-host-cli 迁移
pub mod bilibili;
pub mod time;
pub mod volume;

use tokio::sync::mpsc;

/// Display update message from a provider.
pub struct DisplayUpdate {
    pub slot: u8,
    pub data: DisplayData,
}

pub enum DisplayData {
    Text(String),
    Numeric(i32),
    Progress(u8),
}

/// Trait for data providers that push display updates.
pub trait Provider: Send {
    fn name(&self) -> &str;

    /// The function bitmask bit that enables this provider.
    fn function_bit(&self) -> u16;

    /// Start the provider, sending updates to `tx`.
    fn start(
        &mut self,
        tx: mpsc::Sender<DisplayUpdate>,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}
