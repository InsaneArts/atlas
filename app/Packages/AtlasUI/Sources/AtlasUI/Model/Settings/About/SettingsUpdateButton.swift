/// About's main button, as a check for updates goes: Check for Updates, then Install Update… once a newer version is
/// found, each waiting while it runs.
public enum SettingsUpdateButton: Equatable, Sendable {
  /// Check for Updates.
  case check
  /// Checking…, waiting.
  case checking
  /// Install Update…, which asks first.
  case install
  /// Installing…, waiting.
  case installing
}
