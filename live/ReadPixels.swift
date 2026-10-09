import Foundation
import Vision
import AppKit
let url = URL(fileURLWithPath:CommandLine.arguments[1])
let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
try VNImageRequestHandler(url:url).perform([request])
let result = (request.results ?? []).compactMap { r -> [String:Any]? in
 guard let text = r.topCandidates(1).first else { return nil }
 return ["text":text.string,"confidence":text.confidence,"x":r.boundingBox.minX,"y":r.boundingBox.minY,"w":r.boundingBox.width,"h":r.boundingBox.height]
}
let bytes = try JSONSerialization.data(withJSONObject:result,options:[.sortedKeys]);print(String(decoding:bytes,as:UTF8.self))
