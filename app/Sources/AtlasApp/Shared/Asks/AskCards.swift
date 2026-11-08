import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

/// The questions and grants as the panel and the viewer draw them, from the store and the status: one place, so the
/// two agree. Pure: the panel's and the viewer's presenters call it.
enum AskCards {
  /// The oldest question up, as its card and banner draw it.
  ///
  /// - Parameters:
  ///   - status: The latest status.
  ///   - list: What the store holds; nil before it read.
  ///   - pictures: The questions' pictures, by their file name.
  ///   - date: The time now.
  /// - Returns: The question, or nil for none.
  static func card(
    status: ViewerStatus,
    list: AskList?,
    pictures: [String: DesktopPicture],
    at date: Date
  ) -> AskCardState? {
    guard let ask = StatusReading(status).asks(from: list, at: date).first else { return nil }
    return card(ask, status: status, picture: ask.picture.flatMap { pictures[$0] }, at: date)
  }

  /// One question as its card and banner draw it.
  ///
  /// - Parameters:
  ///   - ask: The question.
  ///   - status: The latest status, for the agent's mark.
  ///   - picture: The question's picture, when it was read.
  ///   - date: The time now.
  /// - Returns: The question's state.
  static func card(_ ask: Ask, status: ViewerStatus, picture: DesktopPicture?, at date: Date) -> AskCardState {
    let title = AskWords.title(ask)
    let host = status.overview.hostSessions.first { $0.key == ask.session }?.host ?? .terminal
    var choices: [AskChoice] = []
    if ask.offersTask { choices.append(.task) }
    if ask.offersOnce { choices.append(.once) }
    return AskCardState(
      id: ask.id,
      icon: host.icon,
      who: ask.who,
      waited: AskWords.waited(date.timeIntervalSince(ask.asked)),
      title: title,
      task: ask.task.map(AskWords.task),
      body: body(ask, picture: picture),
      choices: choices,
      scope: ask.offersTask ? AskWords.scope(agent: AskWords.agent(ask.who), account: ask.account) : nil,
      label: AskWords.label(who: ask.who, title: title)
    )
  }

  /// What a question shows of what would be sent: the message, a password's length, else the element ringed in its
  /// picture.
  ///
  /// - Parameters:
  ///   - ask: The question.
  ///   - picture: The question's picture, when it was read.
  /// - Returns: The body, or nil for none.
  static func body(_ ask: Ask, picture: DesktopPicture?) -> AskCardBody? {
    if ask.kind == .password { return .note(AskWords.password(ask.typed)) }
    if let message = ask.message {
      let cut = message.length > message.preview.count
      return .message(cut ? message.preview + "…" : message.preview)
    }
    guard let picture, let frame = ask.frame else { return nil }
    // The frame is in the picture's own pixels, measured against the size published with it, else the picture's.
    let width = ask.pictureSize?.width ?? Double(picture.image.width)
    let height = ask.pictureSize?.height ?? Double(picture.image.height)
    guard width > 0, height > 0 else { return nil }
    let element = CGRect(
      x: frame.x / width,
      y: frame.y / height,
      width: frame.width / width,
      height: frame.height / height
    )
    return .ringed(picture, element: element)
  }

  /// What the person allowed that still holds, each as its row: the account, the agent and the time left.
  ///
  /// - Parameters:
  ///   - status: The latest status, for the agents' names.
  ///   - list: What the store holds; nil before it read.
  ///   - date: The time now.
  /// - Returns: The rows.
  static func allowed(status: ViewerStatus, list: AskList?, at date: Date) -> [AllowedRowState] {
    StatusReading(status)
      .allowed(from: list, at: date)
      .map { grant in
        let session = status.overview.hostSessions.first { $0.key == grant.session }
        let agent = session.map { $0.host.name(label: $0.hostLabel, session: $0.name) } ?? "An agent"
        return AllowedRowState(
          id: grant.id,
          title: AskWords.allowed(in: grant.name),
          detail: AskWords.timeLeft(agent: agent, seconds: grant.ends.timeIntervalSince(date)),
          endLabel: AskWords.end(grant.name)
        )
      }
  }
}
