use runtime::Runtime;

pub trait RuntimeSelector {
    fn get_by_hostname(&self, hostname: &str) -> Result<Option<&mut Runtime>, SelectorError>;
}

#[derive(Debug)]
pub enum SelectorError {
    Unknown,
    Failure(String),
}
