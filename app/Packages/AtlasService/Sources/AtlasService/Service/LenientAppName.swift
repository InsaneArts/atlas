/// One app a list names, such as the apps a quit step quits or a cleanup leaves running: its name, beside fields
/// this app does not read.
struct LenientAppName: Decodable {
  /// The app's name; nil when the entry names none.
  let app: String?
}
