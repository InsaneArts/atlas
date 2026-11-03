import Foundation

/// Metadata as the pipe accepts it: one bounded JSON object, read in a lexical pass before any Foundation decoding, so
/// duplicate keys, escaped duplicates, invalid escapes and lone surrogates are refused rather than silently merged.
struct FrameJSONValidator {
  /// The bytes read.
  private let bytes: [UInt8]
  /// The next byte's position.
  private var at = 0
  /// The keys read so far, nested ones included.
  private var keys = 0

  private init(_ bytes: [UInt8]) {
    self.bytes = bytes
  }

  /// The top-level keys of `data` and the kind of each value, once it proves one JSON object within the pipe's
  /// bounds, with no key twice in any object, however it is escaped, and nothing after it.
  ///
  /// - Parameter data: The metadata.
  /// - Returns: Each top-level key, decoded, with its value's kind.
  /// - Throws: ``FramePipeError/oversized`` past a bound, else ``FramePipeError/invalid``.
  static func validate(_ data: Data) throws(FramePipeError) -> [String: FrameJSONKind] {
    guard data.count <= FramePipeLimits.metadata else { throw .oversized }
    guard String(data: data, encoding: .utf8) != nil else { throw .invalid }
    var validator = Self(Array(data))
    validator.space()
    guard validator.peek() == UInt8(ascii: "{") else { throw .invalid }
    var top: [String: FrameJSONKind] = [:]
    _ = try validator.object(depth: 1, top: &top)
    validator.space()
    guard validator.at == validator.bytes.count else { throw .invalid }
    return top
  }

  private func peek() -> UInt8? {
    at < bytes.count ? bytes[at] : nil
  }

  private mutating func bump() throws(FramePipeError) -> UInt8 {
    guard let byte = peek() else { throw .invalid }
    at += 1
    return byte
  }

  private mutating func expect(_ byte: Character) throws(FramePipeError) {
    guard try bump() == byte.asciiValue else { throw .invalid }
  }

  private mutating func space() {
    while let byte = peek(), [0x20, 0x09, 0x0A, 0x0D].contains(byte) {
      at += 1
    }
  }

  private mutating func value(depth: Int) throws(FramePipeError) -> FrameJSONKind {
    var nested: [String: FrameJSONKind] = [:]
    guard let byte = peek() else { throw .invalid }
    switch byte {
    case UInt8(ascii: "{"): return try object(depth: depth + 1, top: &nested)
    case UInt8(ascii: "["): return try array(depth: depth + 1)
    case UInt8(ascii: "\""):
      _ = try string(limit: FramePipeLimits.string)
      return .string
    case UInt8(ascii: "t"): return try literal("true", kind: .bool)
    case UInt8(ascii: "f"): return try literal("false", kind: .bool)
    case UInt8(ascii: "n"): return try literal("null", kind: .null)
    case UInt8(ascii: "-"), UInt8(ascii: "0")...UInt8(ascii: "9"): return try number()
    default: throw .invalid
    }
  }

  /// An object; its keys and their values' kinds go to `top`, which only the outermost object keeps.
  private mutating func object(depth: Int, top: inout [String: FrameJSONKind]) throws(FramePipeError)
    -> FrameJSONKind
  {
    guard depth <= FramePipeLimits.depth else { throw .oversized }
    try expect("{")
    var seen: Set<String> = []
    space()
    if peek() == UInt8(ascii: "}") {
      at += 1
      return .object
    }
    while true {
      space()
      let key = try string(limit: FramePipeLimits.key)
      keys += 1
      guard keys <= FramePipeLimits.keys else { throw .oversized }
      guard seen.insert(key).inserted else { throw .invalid }
      space()
      try expect(":")
      space()
      let kind = try value(depth: depth)
      if depth == 1 {
        top[key] = kind
      }
      space()
      switch try bump() {
      case UInt8(ascii: ","): continue
      case UInt8(ascii: "}"): return .object
      default: throw .invalid
      }
    }
  }

  private mutating func array(depth: Int) throws(FramePipeError) -> FrameJSONKind {
    guard depth <= FramePipeLimits.depth else { throw .oversized }
    try expect("[")
    space()
    if peek() == UInt8(ascii: "]") {
      at += 1
      return .array
    }
    while true {
      space()
      _ = try value(depth: depth)
      space()
      switch try bump() {
      case UInt8(ascii: ","): continue
      case UInt8(ascii: "]"): return .array
      default: throw .invalid
      }
    }
  }

  private mutating func literal(_ word: String, kind: FrameJSONKind) throws(FramePipeError) -> FrameJSONKind {
    let spelled = Array(word.utf8)
    guard at + spelled.count <= bytes.count, Array(bytes[at..<(at + spelled.count)]) == spelled else {
      throw .invalid
    }
    at += spelled.count
    return kind
  }

  /// A JSON number: an optional minus, an integer without leading zeros, then an optional fraction and exponent.
  private mutating func number() throws(FramePipeError) -> FrameJSONKind {
    var integer = true
    if peek() == UInt8(ascii: "-") {
      at += 1
      integer = false
    }
    switch try bump() {
    case UInt8(ascii: "0"): break
    case UInt8(ascii: "1")...UInt8(ascii: "9"): digits()
    default: throw .invalid
    }
    if peek() == UInt8(ascii: ".") {
      at += 1
      integer = false
      try digit()
      digits()
    }
    if peek() == UInt8(ascii: "e") || peek() == UInt8(ascii: "E") {
      at += 1
      integer = false
      if peek() == UInt8(ascii: "+") || peek() == UInt8(ascii: "-") {
        at += 1
      }
      try digit()
      digits()
    }
    return integer ? .integer : .number
  }

  private mutating func digit() throws(FramePipeError) {
    guard (UInt8(ascii: "0")...UInt8(ascii: "9")).contains(try bump()) else { throw .invalid }
  }

  private mutating func digits() {
    while let byte = peek(), (UInt8(ascii: "0")...UInt8(ascii: "9")).contains(byte) {
      at += 1
    }
  }

  /// A string, decoded, refused past `limit` decoded UTF-8 bytes.
  private mutating func string(limit: Int) throws(FramePipeError) -> String {
    try expect("\"")
    var decoded: [UInt8] = []
    while true {
      let byte = try bump()
      switch byte {
      case UInt8(ascii: "\""):
        guard let text = String(bytes: decoded, encoding: .utf8) else { throw .invalid }
        return text
      case UInt8(ascii: "\\"):
        decoded.append(contentsOf: try escape())
      case 0..<0x20:
        throw .invalid
      default:
        // The input is valid UTF-8, so continuation bytes follow their lead byte here.
        decoded.append(byte)
      }
      guard decoded.count <= limit else { throw .oversized }
    }
  }

  /// The UTF-8 bytes of the escape after a backslash.
  private mutating func escape() throws(FramePipeError) -> [UInt8] {
    let byte = try bump()
    let simple: [UInt8: UInt8] = [
      UInt8(ascii: "\""): 0x22, UInt8(ascii: "\\"): 0x5C, UInt8(ascii: "/"): 0x2F, UInt8(ascii: "b"): 0x08,
      UInt8(ascii: "f"): 0x0C, UInt8(ascii: "n"): 0x0A, UInt8(ascii: "r"): 0x0D, UInt8(ascii: "t"): 0x09,
    ]
    if let plain = simple[byte] {
      return [plain]
    }
    guard byte == UInt8(ascii: "u") else { throw .invalid }
    return Array(String(try unicode()).utf8)
  }

  /// The character after `\u`: four hex digits, or a surrogate pair of two escapes. A lone surrogate is refused.
  private mutating func unicode() throws(FramePipeError) -> Character {
    let high = try hex4()
    guard !(0xDC00...0xDFFF).contains(high) else { throw .invalid }
    guard (0xD800...0xDBFF).contains(high) else {
      guard let scalar = Unicode.Scalar(high) else { throw .invalid }
      return Character(scalar)
    }
    try expect("\\")
    try expect("u")
    let low = try hex4()
    guard (0xDC00...0xDFFF).contains(low),
      let scalar = Unicode.Scalar(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))
    else { throw .invalid }
    return Character(scalar)
  }

  private mutating func hex4() throws(FramePipeError) -> UInt32 {
    var value: UInt32 = 0
    for _ in 0..<4 {
      guard let digit = Character(Unicode.Scalar(try bump())).hexDigitValue else { throw .invalid }
      value = value * 16 + UInt32(digit)
    }
    return value
  }
}
