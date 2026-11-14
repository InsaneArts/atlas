extension ViewerService {
  /// What changed, as a service's feed says it; one that keeps no feed says nothing, and its readers keep their pace.
  public var changes: ServiceChanges { ServiceChanges.unfollowed }
}
