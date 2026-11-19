extension VMFacts {
  /// A step of a VM's add, as its lifecycle record names it once done.
  enum Step: String, Decodable, Sendable {
    /// Its runner's agent is loaded.
    case agent
    /// Atlas VM made it, macOS installed.
    case created
    /// Atlas VM started it.
    case started
    /// Its enrolment is kept.
    case enrolled
    /// Atlas in its guest reached the close of its install.
    case installed
    /// Its settings entry is written.
    case listed
    /// Its keeper's agent is loaded.
    case keeper
    /// Its guest paired.
    case paired
    /// Its guest was found closed to all but Atlas.
    case closed
    /// Its window opened on this Mac's screen for the person to allow Atlas there.
    case shown
    /// Both permissions are allowed in its guest.
    case allowed
    /// Its window takes no input any more.
    case hidden
    /// Its setup is done, and agents wait for the person's Resume.
    case ready
    /// The add's own Resume let agents go on.
    case resumed
  }
}
