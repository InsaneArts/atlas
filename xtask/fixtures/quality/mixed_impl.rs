struct Example;
impl Example {
    fn production() {}
    /// A test-only constructor.
    #[cfg(test)]
    fn fixture() {
        storage().unwrap();
    }
    /// A test-only constant.
    #[cfg(test)]
    const FIXTURE: bool = true;
}
