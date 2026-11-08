/// One question as a notification shows it: its question, who asks and the task, and which answers it carries.
struct AskNote: Equatable {
  /// The question's id, which an answer names and the notification is kept under.
  var id: String
  /// The question, such as "Send a message in Slack?".
  var title: String
  /// Who asks and the task.
  var body: String
  /// The notification's kind, by the answers it offers (``AskNotifications/category(task:once:)``).
  var category: String
}
