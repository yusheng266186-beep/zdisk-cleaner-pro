use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("模式编译失败 [{pattern}]: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: globset::Error,
    },

    /// 安全守卫拒绝了操作。任何路径解析失败都按违规处理（fail-closed）。
    #[error("安全守卫拒绝路径 {path}: {reason}")]
    GuardRejected { path: String, reason: String },

    #[error("外部命令执行失败: {0}")]
    External(String),

    #[error("{0}")]
    Other(String),
}
