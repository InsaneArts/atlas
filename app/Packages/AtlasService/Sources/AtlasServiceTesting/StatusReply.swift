import AtlasService
import Foundation

/// A status reply as the viewer service writes it.
///
/// - Parameter reply: The status's JSON fields.
/// - Returns: The decoded status.
/// - Throws: A decoding error when the fields are not a status.
public func status(_ reply: [String: Any]) throws -> ViewerStatus {
  let decoder = JSONDecoder()
  return try decoder.decode(ViewerStatus.self, from: JSONSerialization.data(withJSONObject: reply))
}

/// One of the overview's `hostSessions` as the viewer service writes it, for a status reply. Only the fields given
/// are written, so a test of the defaults leaves them out.
///
/// - Parameters:
///   - key: The session's key; empty for Terminal.
///   - host: The host's word, such as `claude`.
///   - name: The session's name.
///   - state: Its state's word; idle by default.
///   - hostLabel: The host's name in the service's words; none by default.
///   - holdsHelper: Whether it has the helper.
///   - waitingFor: What it waits for: `helper` or `recovery`.
///   - waitingSeconds: How long it has waited for the helper.
///   - latest: Its newest action, as an `activity` row's fields.
///   - uncertainRow: The row id of its action of unknown outcome.
///   - uncertainVerb: That action's verb.
/// - Returns: The entry's JSON fields.
public func hostSession(
  _ key: String,
  host: String,
  name: String,
  state: String = "idle",
  hostLabel: String? = nil,
  holdsHelper: Bool = false,
  waitingFor: String? = nil,
  waitingSeconds: Double? = nil,
  latest: [String: Any]? = nil,
  uncertainRow: String? = nil,
  uncertainVerb: String? = nil
) -> [String: Any] {
  var fields: [String: Any] = [
    "key": key, "host": host, "name": name, "state": state, "live": state != "ended", "holdsHelper": holdsHelper,
  ]
  fields["hostLabel"] = hostLabel
  fields["waitingFor"] = waitingFor
  fields["waitingSeconds"] = waitingSeconds
  fields["latest"] = latest
  fields["uncertainRow"] = uncertainRow
  fields["uncertainVerb"] = uncertainVerb
  return fields
}
