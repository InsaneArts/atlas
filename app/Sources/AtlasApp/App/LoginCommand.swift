import AtlasKit
import Foundation

/// `--login on|off|status`: manages Open at Login for the command line, answering in JSON on standard output.
@MainActor
enum LoginCommand {
  /// Applies `change` and prints the result, then exits.
  ///
  /// - Parameters:
  ///   - change: `on`, `off`, or anything else to report alone.
  ///   - loginItem: The app's login item.
  /// - Returns: Never; the process exits.
  static func run(_ change: String, loginItem: OpenAtLogin) -> Never {
    do {
      switch change {
      case "on": try loginItem.set(true)
      case "off": try loginItem.set(false)
      default: break
      }
      answer(.state(loginItem.state))
      exit(0)
    } catch {
      answer(.failure(error.localizedDescription))
      exit(1)
    }
  }

  /// Writes the reply, a line of JSON: it is this command's protocol with `atlas menubar --login`, so it goes to
  /// standard output.
  ///
  /// - Parameter reply: The reply.
  private static func answer(_ reply: LoginReply) {
    FileHandle.standardOutput
      .write(reply.json + Data("\n".utf8))
  }
}
