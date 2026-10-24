import AtlasKit

extension StatusRow.Detail {
  /// What the check found, as the Ready card says it: a silent helper's overview reason stands for its failed checks,
  /// whose steps name commands.
  var word: String {
    switch self {
    case .passed: CheckWords.ready
    case .notKnown: CheckWords.notKnown
    case .failed(let check): CheckWords.failed(check)
    case .helperSilent(let reason): reason ?? Words.connecting
    case .worked(let age): CheckWords.worked(ageSeconds: age)
    case .didNotWork(let age): CheckWords.didNotWork(ageSeconds: age)
    case .notChecked: CheckWords.notCheckedYet
    }
  }
}
