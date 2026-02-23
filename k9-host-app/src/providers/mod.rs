// INPUT:  tokio, anyhow
// OUTPUT: Provider trait + DisplayUpdate/DisplayData 类型 + 四个具体 provider 子模块
// POS:    数据提供者抽象层 — 定义统一接口，子模块各自实现具体数据源
pub mod ai_quota;
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
