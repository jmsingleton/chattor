use crate::error::Result;

pub struct App {
    // Will hold application state
}

impl App {
    pub fn new() -> Result<Self> {
        Ok(App {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert!(app.is_ok());
    }
}
