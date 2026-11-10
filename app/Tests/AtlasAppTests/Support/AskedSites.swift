/// The sites whose icon a stand-in was asked for, in order.
actor AskedSites {
  /// The sites so far.
  var sites: [String] = []

  /// Notes that `site` was asked for.
  ///
  /// - Parameter site: The site.
  func note(_ site: String) { sites.append(site) }
}
