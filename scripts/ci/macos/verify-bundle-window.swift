import CoreGraphics
import Darwin
import Foundation

// 起動から操作可能なウィンドウが現れるまでの時間を計測する（tasks.md 10.3）。
//
// アプリの起動は**このスパイク自身**が行う。計測区間の始点を起動の瞬間に
// 取れるのはここだけである。シェルが `nohup ... &` してから `swift`
// （インタプリタ）の起動を待って計時を始めると、アプリが先に動き出した分
// だけ計測値が**小さく**出る（＝予算判定が甘くなる）。逆にシェル側で
// `swift` の前後を計るとインタプリタの起動時間が混ざって**大きく**出る。
// 始点は `Process.run()` の直前、終点は対象 pid の通常レイヤー（0）の
// 100x100 以上のウィンドウを観測した時刻である。観測は 0.1 秒ごと。
let appPath = CommandLine.arguments[1]
let logPath = CommandLine.arguments[2]
let measurePath = CommandLine.arguments[3]
let timeoutSeconds = 60.0

_ = FileManager.default.createFile(atPath: logPath, contents: nil)
let process = Process()
process.executableURL = URL(fileURLWithPath: appPath)
if let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: logPath)) {
    process.standardOutput = handle
    process.standardError = handle
}

let start = Date()
do {
    try process.run()
} catch {
    FileHandle.standardError.write("NG: アプリを起動できません: \(error)\n".data(using: .utf8)!)
    exit(1)
}
let pid = process.processIdentifier
let deadline = start.addingTimeInterval(timeoutSeconds)

func stop() {
    process.terminate()
    Thread.sleep(forTimeInterval: 0.5)
    if process.isRunning { kill(process.processIdentifier, SIGKILL) }
}

while Date() < deadline {
    if !process.isRunning {
        FileHandle.standardError.write("NG: アプリがウィンドウを出す前に終了しました\n".data(using: .utf8)!)
        exit(1)
    }
    if let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] {
        for w in list {
            guard let owner = w[kCGWindowOwnerPID as String] as? Int, owner == Int(pid) else { continue }
            guard let layer = w[kCGWindowLayer as String] as? Int, layer == 0 else { continue }
            guard let b = w[kCGWindowBounds as String] as? [String: Any],
                  let width = b["Width"] as? Double, let height = b["Height"] as? Double,
                  width >= 100, height >= 100 else { continue }
            // 計測値は「観測した時刻」なので、実際の表示より最大 0.1 秒
            // 大きく出る（保守側）。
            let elapsedMs = Int(Date().timeIntervalSince(start) * 1000)
            try? "macos=\(elapsedMs)\n".write(toFile: measurePath, atomically: true, encoding: .utf8)
            print("OK: ウィンドウ \(Int(width))x\(Int(height)) が現れました (pid=\(pid), 起動から \(elapsedMs) ms)")
            print("計測値: macos=\(elapsedMs)（書き出し先 \(measurePath)）")
            stop()
            exit(0)
        }
    }
    Thread.sleep(forTimeInterval: 0.1)
}
FileHandle.standardError.write("NG: \(Int(timeoutSeconds)) 秒以内にウィンドウが現れませんでした\n".data(using: .utf8)!)
stop()
exit(1)
