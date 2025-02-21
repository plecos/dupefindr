#[derive(Debug, PartialEq)]
pub enum InteractiveErrorKind {
    Skip,
    Escape,
    Other,
}

#[derive(Debug, thiserror::Error)]
pub enum InteractiveError {
    #[error("Skip")]
    Skip(),

    #[error("Escape")]
    Escape(),

    #[error("Other: {0}")]
    Other(String),
}

impl InteractiveError {
    pub fn kind(&self) -> InteractiveErrorKind {
        match self {
            InteractiveError::Skip() => InteractiveErrorKind::Skip,
            InteractiveError::Escape() => InteractiveErrorKind::Escape,
            InteractiveError::Other(_) => InteractiveErrorKind::Other,
        }
    }
}
