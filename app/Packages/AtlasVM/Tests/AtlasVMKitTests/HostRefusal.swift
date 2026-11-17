/// A host's refusal, said in the words it was given, as Atlas VM's own
/// errors are.
struct HostRefusal: Error, CustomStringConvertible {
  let description: String
}
