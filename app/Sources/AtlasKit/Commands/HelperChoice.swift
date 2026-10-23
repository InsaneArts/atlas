/// What `atlas onboarding create` and `atlas onboarding choose` report: whether the helper was readied for its
/// first login, so Atlas opens there by itself.
package struct HelperChoice: Decodable, Sendable {
  /// Whether it was readied; false when this macOS could not ready it, so the person opens Atlas there.
  package let readied: Bool?
}
