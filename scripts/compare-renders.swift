// Counts the pixels that differ between pairs of PNGs, for compare-renders.mjs:
//   swift scripts/compare-renders.swift [--diff-dir DIR] A1 B1 NAME1 [A2 B2 NAME2 ...]
// Prints one line a pair, in order: its index from 0, a tab, then the count
// of differing pixels, or "size WxH vs WxH", or which side is unreadable.
// With --diff-dir, each pair with differing pixels also gets
// DIR/diff-NAME.png: changed pixels magenta on a faded grey copy of A. NAME
// must be a plain file name; nothing is written outside DIR.
import CoreGraphics
import Foundation
import ImageIO

struct Pixels {
  let width: Int
  let height: Int
  let bytes: [UInt8]
}

func decode(_ path: String) -> Pixels? {
  guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
    let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
  else { return nil }
  let width = image.width
  let height = image.height
  var bytes = [UInt8](repeating: 0, count: width * height * 4)
  let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
    guard
      let context = CGContext(
        data: buffer.baseAddress,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )
    else { return false }
    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    return true
  }
  return drawn ? Pixels(width: width, height: height, bytes: bytes) : nil
}

func writeDiff(_ a: Pixels, changed: [Bool], to url: URL) -> Bool {
  var out = [UInt8](repeating: 255, count: a.bytes.count)
  for index in 0..<changed.count {
    let at = index * 4
    if changed[index] {
      out[at] = 255
      out[at + 1] = 0
      out[at + 2] = 255
    } else {
      let r = Double(a.bytes[at])
      let g = Double(a.bytes[at + 1])
      let b = Double(a.bytes[at + 2])
      let grey = 0.299 * r + 0.587 * g + 0.114 * b
      let faded = UInt8((255 - (255 - grey) * 0.25).rounded())
      out[at] = faded
      out[at + 1] = faded
      out[at + 2] = faded
    }
  }
  let image = out.withUnsafeMutableBytes { buffer -> CGImage? in
    CGContext(
      data: buffer.baseAddress,
      width: a.width,
      height: a.height,
      bitsPerComponent: 8,
      bytesPerRow: a.width * 4,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )?
    .makeImage()
  }
  guard let image,
    let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil)
  else { return false }
  CGImageDestinationAddImage(destination, image, nil)
  return CGImageDestinationFinalize(destination)
}

func compare(_ pathA: String, _ pathB: String, name: String, diffDir: URL?) -> String {
  let a = decode(pathA)
  let b = decode(pathB)
  guard let a, let b else {
    return a == nil && b == nil ? "unreadable A and B" : a == nil ? "unreadable A" : "unreadable B"
  }
  guard a.width == b.width, a.height == b.height else {
    return "size \(a.width)x\(a.height) vs \(b.width)x\(b.height)"
  }
  var changed = [Bool](repeating: false, count: a.width * a.height)
  var count = 0
  for index in 0..<changed.count {
    let at = index * 4
    if a.bytes[at] != b.bytes[at] || a.bytes[at + 1] != b.bytes[at + 1] || a.bytes[at + 2] != b.bytes[at + 2]
      || a.bytes[at + 3] != b.bytes[at + 3]
    {
      changed[index] = true
      count += 1
    }
  }
  if count > 0, let diffDir {
    let url = diffDir.appendingPathComponent("diff-\(name).png")
    // Only a plain file name inside the folder: never a path out of it.
    guard !name.contains("/"), url.deletingLastPathComponent().standardized == diffDir.standardized,
      writeDiff(a, changed: changed, to: url)
    else { return "\(count) (diff not written)" }
  }
  return "\(count)"
}

var arguments = Array(CommandLine.arguments.dropFirst())
var diffDir: URL?
if arguments.first == "--diff-dir", arguments.count >= 2 {
  diffDir = URL(fileURLWithPath: arguments[1], isDirectory: true)
  arguments.removeFirst(2)
}
guard !arguments.isEmpty, arguments.count % 3 == 0 else {
  FileHandle.standardError.write(Data("usage: compare-renders.swift [--diff-dir DIR] A B NAME ...\n".utf8))
  exit(2)
}
for start in stride(from: 0, to: arguments.count, by: 3) {
  let result = compare(arguments[start], arguments[start + 1], name: arguments[start + 2], diffDir: diffDir)
  print("\(start / 3)\t\(result)")
}
