import AtlasService
import Foundation

/// Waiting for you: the questions Atlas asks the person before an agent sends, and what they allowed for a task. The
/// panel, the viewer and the notification show the same ones: those `asks.json` last gave, which carry what each would
/// send, or, before it was read, the status's own summary of them.
extension StatusReading {
  /// The questions up at `date`, oldest first: `list`'s once it was read, else the status's.
  ///
  /// - Parameters:
  ///   - list: What `asks.json` last said; nil before it was read.
  ///   - date: The time now.
  /// - Returns: The questions not ended yet.
  public func asks(from list: AskList?, at date: Date) -> [Ask] {
    (list?.asks ?? status.overview.asks)
      .filter { $0.isOpen(at: date) }
  }

  /// Whether a question waits for the person at `date`: the state beside Needs you that the panel, the viewer and the
  /// menu bar show as Waiting for you.
  ///
  /// - Parameters:
  ///   - list: What `asks.json` last said; nil before it was read.
  ///   - date: The time now.
  /// - Returns: Whether any question is up.
  public func waitsForYou(from list: AskList?, at date: Date) -> Bool {
    !asks(from: list, at: date).isEmpty
  }

  /// What the person allowed for a task that still holds at `date`: `list`'s once it was read, else the status's.
  ///
  /// - Parameters:
  ///   - list: What `asks.json` last said; nil before it was read.
  ///   - date: The time now.
  /// - Returns: The grants not ended yet.
  public func allowed(from list: AskList?, at date: Date) -> [AllowedGrant] {
    (list?.allowed ?? status.overview.allowed)
      .filter { $0.ends > date }
  }

  /// Whether `asks.json` should be read again: the status lists other questions or grants than `list` holds, as when
  /// one came or went while the watch was not looking, or a grant ended with its agent session.
  ///
  /// - Parameter list: What `asks.json` last said; nil before it was read.
  /// - Returns: Whether the two disagree.
  public func asksChanged(since list: AskList?) -> Bool {
    guard let list else { return true }
    return Set(status.overview.asks.map(\.id)) != Set(list.asks.map(\.id))
      || Set(status.overview.allowed.map(\.id)) != Set(list.allowed.map(\.id))
  }
}
