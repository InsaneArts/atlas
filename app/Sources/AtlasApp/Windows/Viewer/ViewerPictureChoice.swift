/// The ⋯ menu's picture choices, by the tag their items carry.
enum ViewerPictureChoice: Int, CaseIterable {
  /// Pictures as often as the window reads them.
  case live
  /// A picture every five seconds.
  case everyFiveSeconds
  /// No new pictures.
  case paused
}
