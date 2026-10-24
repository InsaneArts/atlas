extension SettingsHelperState {
  /// A live summary of reported checks without inferring readiness from a single capability.
  public var readinessSummary: String {
    if buttons.isChecking { return "Checking…" }
    guard !ready.isEmpty else { return "Not checked yet" }
    let failed = ready.filter { $0.item.state == .notReady }.count
    let unknown = ready.filter { $0.item.state == .unknown }.count
    if failed == 0 && unknown == 0 { return "All reported checks passed" }
    return [
      failed > 0 ? "\(failed) needing attention" : nil,
      unknown > 0 ? "\(unknown) not checked" : nil,
    ]
    .compactMap { $0 }
    .joined(separator: " · ")
  }
}
