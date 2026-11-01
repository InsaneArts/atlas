trait Private {
    fn signature(&self);
    fn body(&self) {}
    /// A test-only default.
    #[cfg(test)]
    fn fixture(&self) {}
}
pub trait Public {
    fn signature(&self);
    fn body(&self) {}
}
