/// What made an install stop for good, rather than wait to be asked again.
struct InstallRefusal: Error {
  /// Why, as the person reads it.
  let why: String
}
