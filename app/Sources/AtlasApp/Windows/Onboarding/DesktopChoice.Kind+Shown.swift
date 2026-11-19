import AtlasOnboarding
import AtlasUI

extension DesktopChoice.Kind {
  /// The kind the window's choice names `kind`.
  ///
  /// - Parameter kind: The kind as the window shows it.
  init(_ kind: OnboardingDesktopKind) {
    switch kind {
    case .secondLogin: self = .secondLogin
    case .vm: self = .vm
    case .cloud: self = .cloud
    }
  }

  /// The kind as the window's choice shows it.
  var shown: OnboardingDesktopKind {
    switch self {
    case .secondLogin: .secondLogin
    case .vm: .vm
    case .cloud: .cloud
    }
  }
}
