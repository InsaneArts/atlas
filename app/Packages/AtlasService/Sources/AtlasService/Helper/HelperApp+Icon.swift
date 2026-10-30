extension HelperApp {
  /// What the app's icon is known by: its bundle identifier, else its name; nil for an app with neither.
  public var iconKey: AppIconKey? {
    AppIconKey(bundleId: bundleId) ?? AppIconKey(name: name)
  }

  /// The app's bundle, when it is installed where apps are, so its icon can be read on the person's side; nil for a
  /// copy only the helper can read or a bundle anywhere else.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The bundle's path, or nil.
  public func installedBundle(home: String) -> String? {
    guard let bundle, bundle.hasSuffix(TrustedAppRoots.bundleSuffix), TrustedAppRoots.contains(bundle, home: home)
    else { return nil }
    return bundle
  }
}
