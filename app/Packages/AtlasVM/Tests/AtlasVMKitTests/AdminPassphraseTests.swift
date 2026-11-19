import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A new VM's administrator password")
struct AdminPassphraseTests {
  @Test("a new VM's administrator password is four groups of five letters typed alike on common layouts")
  func typable() {
    var generator = SystemRandomNumberGenerator()
    var made: [String] = []
    for _ in 0..<64 {
      made.append(AdminPassphrase.make(using: &generator))
    }

    for password in made {
      #expect(password.count == 20)
      let letters = AdminPassphrase.letters
      #expect(password.allSatisfy { letter in letters.contains(letter) })
      let groups = AdminPassphrase.grouped(password)
      #expect(groups.count == 4)
      #expect(groups.allSatisfy { group in group.count == 5 })
    }
    #expect(Set(made).count == made.count, "drawn at random each time")
    #expect(AdminPassphrase.letters.count == 16, "four bits a letter, 80 bits in all")
    // Letters QWERTY, QWERTZ and AZERTY keep on another key, and those read
    // as another, are never used.
    #expect(AdminPassphrase.letters.allSatisfy { !"aqwzymilo".contains($0) })
  }

  @Test("an older VM's password, not one of these, is shown whole")
  func olderWhole() {
    #expect(AdminPassphrase.grouped("ABCdef23") == ["ABCdef23"])
  }
}
