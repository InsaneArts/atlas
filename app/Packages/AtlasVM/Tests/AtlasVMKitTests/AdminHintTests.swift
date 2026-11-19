import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The administrator's row in a VM's window")
struct AdminHintTests {
  let password = "bcdefghjknprstuvbcde"

  @Test("the password row shows only during a new VM's setup window, hidden by default, and Show reveals it")
  func shownOnlyForSetup() throws {
    #expect(AdminHint(setup: false, password: password) == nil, "a maintenance window shows none")
    #expect(AdminHint(setup: true, password: nil) == nil, "none without the password its creation kept")
    var hint = try #require(AdminHint(setup: true, password: password))

    let hidden = hint.shown
    let hiddenButton = hint.button
    hint.toggle()
    let revealed = hint.shown
    let revealedButton = hint.button
    hint.toggle()

    #expect(AdminHint.words.contains("atlas-admin"))
    #expect(AdminHint.typing == "Type it without spaces.")
    #expect(hidden == ["•••••", "•••••", "•••••", "•••••"])
    #expect(hiddenButton == "Show")
    #expect(revealed == ["bcdef", "ghjkn", "prstu", "vbcde"])
    #expect(revealedButton == "Hide")
    #expect(hint.shown == hidden, "Hide hides it again")
  }

  @Test("a hint never says its password when it is described, shown or not")
  func neverDescribed() throws {
    var hint = try #require(AdminHint(setup: true, password: password))
    hint.toggle()

    let described = ["\(hint)", String(describing: hint), String(reflecting: hint)]

    for said in described {
      #expect(!said.contains(password), "\(said)")
      #expect(!said.contains("bcdef"), "\(said)")
    }
  }
}
