import Foundation
import Testing

@testable import AtlasService

@Suite("ActionPicture")
struct ActionPictureTests {
  @Test("a picture before or after an action is known by its own name, which says when it was taken")
  func knownByItsName() {
    let taken = Date(timeIntervalSince1970: 1_700_000_005)
    for name in [
      "1700000005000000000-before.jpg",
      "1700000005000000000-0123456789ab-before.jpg",
      "1700000005000000000-0123456789ab-ba9876543210-before.jpg",
      "1700000005000000000-after.jpg",
      "1700000005000000000-0123456789ab-ba9876543210-after.jpg",
    ] {
      #expect(ActionPicture.takenAt(name) == taken, "\(name)")
    }
    for name in [
      "1700000005000000000-capture.png", "-before.jpg", "17x0-before.jpg", "before/1-before.jpg", "",
      "1--before.jpg", "1-0123456789AB-before.jpg", "1-0123456789ab-0123456789ab-0123456789ab-before.jpg",
      "-after.jpg", "1-during.jpg",
    ] {
      #expect(!ActionPicture.isName(name), "\(name)")
    }
  }

  @Test("only a picture named after an action reads as one taken after it")
  func afterByItsName() {
    #expect(ActionPicture.isAfter("1700000005000000000-after.jpg"))
    #expect(!ActionPicture.isAfter("1700000005000000000-before.jpg"))
    #expect(!ActionPicture.isAfter("x-after.jpg"))
  }
}
