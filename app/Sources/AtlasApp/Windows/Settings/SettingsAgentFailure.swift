/// Why an agent's last connection change failed and which state resolves it.
struct SettingsAgentFailure: Equatable {
  /// The command's failure message, including the Disconnect label when removing a connection.
  var message: String
  /// Whether the failure waits for disconnection rather than connection.
  var isDisconnect = false
}
