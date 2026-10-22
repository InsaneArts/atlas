extension HelperApp {
  /// What the app's icon is known by: its bundle's path, the same at every launch, else its process.
  package var iconID: String { bundle ?? "pid \(pid)" }
}
