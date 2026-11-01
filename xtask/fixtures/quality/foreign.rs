#[allow(unsafe_code)]
mod ffi {
    #[link(name = "fixture")]
    unsafe extern "C" {
        /// A foreign function.
        #[link_name = "foreign_call"]
        pub fn call(value: u8) -> u8;
        static VALUE: u8;
        type Opaque;
        /// A test-only declaration.
        #[cfg(test)]
        fn fixture();
    }
}
