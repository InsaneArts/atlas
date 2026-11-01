struct First { ready: bool, label: &'static str }
struct Second { ready: bool }
fn decisions(first: bool, second: bool) {
    let _ = storage();
    storage().ok();
    storage().unwrap_or_default();
    outer!({ "é": inner!(rows[0]), "other": rows[1] });
    outer!(rows[2]);
}
fn caller() { decisions(true, false); }
#[cfg(not(feature = "active"))]
fn inactive() { storage().expect("local proof needed"); }
impl Drop for First {
    fn drop(&mut self) { storage().ok(); }
}
#[cfg(test)]
impl First {
    fn fixture() { storage().unwrap(); }
}
#[cfg(test)]
mod tests {
    fn timing() { std::thread::sleep(delay); assert!(now.elapsed() < delay); }
}
