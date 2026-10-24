import AtlasStatus

extension StatusRow {
  /// The short line this check shows beside Recover…, when it failed and Recover is its fix; nil otherwise.
  var recoverLine: String? {
    guard case .failed(let check) = detail else { return nil }
    return CheckWords.recoverLine(check)
  }
}
