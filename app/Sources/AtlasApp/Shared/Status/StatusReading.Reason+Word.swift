import AtlasStatus

extension StatusReading.Reason {
  /// The reason as a window gives it: a failed check in plain words, never the command its step names.
  var word: String {
    switch self {
    case .yourControl: Words.yourControl
    case .yourControlBesideCommand: Words.yourControlBesideCommand
    case .inUseElsewhere: Words.inUseElsewhereLine
    case .nothingHappening: Words.nothingHappening
    case .lastAction(let item): Words.idleLine(last: item.text, ago: item.when)
    case .failedCheck(let check): CheckWords.failed(check)
    case .service(let reason): reason
    case .connecting: Words.connecting
    }
  }
}
