/// The icon queue's working parts, unobserved: what waits, what is being found, and what was tried, so an icon is
/// looked for once and a reading that comes every few seconds asks for nothing it asked for before.
struct ViewerIconWork {
  /// Icons asked for and not yet started, oldest first.
  var wanted: [AppIconWant] = []
  /// Icons being found, by their key's text.
  var loading: [String: Task<Void, Never>] = [:]
  /// Keys whose icon was looked for and not found, which stay their tile until the next launch.
  var missed: Set<String> = []
  /// Keys whose open app's icon was read from the app itself, which is done once.
  var readLive: Set<String> = []
  /// The name and bundle identifier pairs already passed on to be kept, by the name's key's text.
  var learned: [String: String] = [:]
}
