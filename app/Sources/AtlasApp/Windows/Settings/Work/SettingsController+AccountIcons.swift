import AtlasService
import AtlasUI

extension SettingsController {
  /// Asks once for the icon of each listed account it has none of: an app's from this Mac at once, a site's from the
  /// site itself, kept when it comes should the account still be listed. An account no longer listed loses its icon,
  /// and is asked for again should it come back.
  ///
  /// - Parameter listed: The accounts listed now.
  func wantAccountIcons(_ listed: [ViewerStatus.Account]) {
    let ids = Set(listed.map(\.id))
    let kept = accountIcons.filter { ids.contains($0.key) }
    if kept.count != accountIcons.count { accountIcons = kept }
    askedIcons.formIntersection(ids)
    for account in listed where askedIcons.insert(account.id).inserted {
      let id = account.id
      switch account.kind {
      case .app:
        guard let image = dependencies.appIcon(account.key) else { continue }
        accountIcons[id] = .app(IconImage(id: id, image: image))
      case .site:
        let read = dependencies.siteIcon
        let site = account.key
        // Unstructured, as the list is taken in synchronously: the icon lands whenever the site answers.
        Task { [weak self] in
          guard let image = await read(site), let self, askedIcons.contains(id) else { return }
          accountIcons[id] = .site(IconImage(id: id, image: image))
        }
      }
    }
  }
}
