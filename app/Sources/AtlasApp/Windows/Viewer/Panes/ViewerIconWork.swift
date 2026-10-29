/// The icon queue's working parts, unobserved: what waits, what is being found, and what was tried, so an icon is
/// looked for once and a reading that comes every few seconds asks for nothing it asked for before.
struct ViewerIconWork {
  /// Icons asked for and not yet started, oldest first.
  var wanted: [AppIconWant] = []
  /// Icons being found, by their key's text.
  var loading: [String: Task<Void, Never>] = [:]
  /// The requests being processed, so stronger requests can wait behind them.
  var active: [String: AppIconWant] = [:]
  /// Keys whose icon was not found, retried when live evidence arrives.
  var missed: Set<String> = []
  /// Live requests already processed; new bundle or service evidence may ask again.
  var readLive: [String: AppIconWant] = [:]
  /// The name and bundle identifier pairs already passed on to be kept, by the name's key's text.
  var learned: [String: String] = [:]
}
