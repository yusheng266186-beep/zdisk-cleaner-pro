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

    /// 操作被用户取消（扫描/去重/雷达等忙任务）。壳层映射 code="cancelled"。
    #[error("已取消: {reason}")]
    Cancelled { reason: String },

    /// 需要管理员权限而当前进程未提权。壳层映射 code="admin_required"。
    #[error("需要管理员权限: {reason}")]
    AdminRequired { reason: String },

    /// 目标不存在（台账批次、路径等）。壳层映射 code="not_found"。
    #[error("未找到: {what}")]
    NotFound { what: String },

    /// 同类忙任务已在运行。壳层映射 code="busy"。
    #[error("忙碌: {reason}")]
    Busy { reason: String },

    /// 资源被占用（文件锁等）。壳层映射 code="locked"。
    #[error("资源被占用: {path}")]
    Locked { path: String },

    #[error("外部命令执行失败: {0}")]
    External(String),

    #[error("{0}")]
    Other(String),
}
