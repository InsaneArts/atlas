extension SettingsAccountRow {
  /// What leads the account's row: its app's own icon, its site's icon on a white tile, or its first letter on its
  /// name's tint, as Atlas draws an app without its own icon.
  var lead: ListRow.Lead {
    switch icon {
    case .app(let image): .appIcon(image)
    case .site(let image): .siteIcon(image)
    case nil: .monogram(AppMonogram(letter: letter, tint: MonogramTint(name: title)))
    }
  }
}
