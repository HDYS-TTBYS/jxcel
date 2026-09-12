import CoreGraphics
import Darwin
import Foundation

// アプリを起動してウィンドウを観測し、診断記録から初回描画（8.2）と**描画された画面**
// （10.4）の行を読む。
// 引数: <実行ファイル> <期待する画面の識別子|空> <記録ファイル> <アプリ出力ファイル> <タイムアウト秒>
//
// **記録とアプリの出力は別のファイルである。**アプリの標準出力・標準誤差を記録へ
// つなぐと、アプリと診断記録（`TargetKind::Folder`）が同じファイルへ独立に書く
// ことになり、アプリ側が先頭から上書きして記録の行を壊しうる。計測（10.3）は
// 1.5 の段が別に行うので、ここでは計時しない。
let appPath = CommandLine.arguments[1]
let expectedScreen = CommandLine.arguments[2]
let recordPath = CommandLine.arguments[3]
let appOutputPath = CommandLine.arguments[4]
let timeoutSeconds = Double(CommandLine.arguments[5]) ?? 60.0

// 描画が成立したことを示す行は 3 つある: `Painted` / ソフトウェアラスタライザ経由 /
// **期限を超えてから成立した行**（下の 3 つ目。判定は `NoPaint` のままだが `画面=` を
// 運ぶので、どの画面が描画されたかの証拠としては同等である）。
let heartbeatPatterns = [
    "初回描画が成立した: label=",
    "初回描画は成立したがソフトウェアラスタライザ経由である: label=",
    // **期限を超えてから描画が成立した場合**（遅いランナー。実測: 期限=3000 ms に対し
    // 経過=3142 ms）。この行も `画面=` を運ぶので、どの画面が描画されたかの証拠と
    // しては同等である（判定は `NoPaint` のままで、不成立の提示だけが取り下げられる）。
    "期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label=",
]
let requestedPattern = "検証用の初期画面を指定した: screen=\(expectedScreen)"

func recordText() -> String {
    return (try? String(contentsOfFile: recordPath, encoding: .utf8)) ?? ""
}

/// 成立行が報告する**実際に描画されていた画面**を取る（`画面=<識別子>`）。
func renderedScreen(in line: String) -> String? {
    guard let range = line.range(of: "画面=") else { return nil }
    let rest = line[range.upperBound...]
    return rest.split(separator: " ", maxSplits: 1).first.map { String($0) }
}

func tail(_ path: String, _ count: Int) -> [String] {
    return ((try? String(contentsOfFile: path, encoding: .utf8)) ?? "")
        .split(separator: "\n").suffix(count).map { String($0) }
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write("NG: \(message)\n".data(using: .utf8)!)
    FileHandle.standardError.write("--- 診断記録（末尾）: \(recordPath) ---\n".data(using: .utf8)!)
    for line in tail(recordPath, 20) {
        FileHandle.standardError.write("\(line)\n".data(using: .utf8)!)
    }
    FileHandle.standardError.write("--- アプリの出力（末尾）: \(appOutputPath) ---\n".data(using: .utf8)!)
    for line in tail(appOutputPath, 40) {
        FileHandle.standardError.write("\(line)\n".data(using: .utf8)!)
    }
    exit(1)
}

// 記録とアプリの出力を空にしてから起動する（前回の起動の行で偽の成功をしない）。
_ = FileManager.default.createFile(atPath: recordPath, contents: nil)
_ = FileManager.default.createFile(atPath: appOutputPath, contents: nil)

let process = Process()
process.executableURL = URL(fileURLWithPath: appPath)
var environment = ProcessInfo.processInfo.environment
if expectedScreen.isEmpty {
    environment.removeValue(forKey: "JXCEL_VERIFICATION_INITIAL_SCREEN")
} else {
    environment["JXCEL_VERIFICATION_INITIAL_SCREEN"] = expectedScreen
}
process.environment = environment
// **アプリの出力は記録とは別のファイルへ。**配布物は環境変数を読まないので要求は
// 無視されるが、期待（初期画面は 9.6 の空ウィンドウの画面）は成立する。
if let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: appOutputPath)) {
    process.standardOutput = handle
    process.standardError = handle
}

let start = Date()
do {
    try process.run()
} catch {
    fail("アプリを起動できません: \(error)")
}
let pid = process.processIdentifier

func stop() {
    process.terminate()
    Thread.sleep(forTimeInterval: 0.5)
    if process.isRunning { kill(process.processIdentifier, SIGKILL) }
}

// 1. ウィンドウの出現（1.5 と同じ観測: 通常レイヤーの 100x100 以上）。
let windowDeadline = start.addingTimeInterval(timeoutSeconds)
var windowLine: String? = nil
while Date() < windowDeadline {
    if !process.isRunning {
        stop()
        fail("アプリがウィンドウを出す前に終了しました")
    }
    if let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] {
        for window in list {
            guard let owner = window[kCGWindowOwnerPID as String] as? Int, owner == Int(pid) else { continue }
            guard let layer = window[kCGWindowLayer as String] as? Int, layer == 0 else { continue }
            guard let bounds = window[kCGWindowBounds as String] as? [String: Any],
                  let width = bounds["Width"] as? Double, let height = bounds["Height"] as? Double,
                  width >= 100, height >= 100 else { continue }
            let elapsedMs = Int(Date().timeIntervalSince(start) * 1000)
            windowLine = "OK: ウィンドウ \(Int(width))x\(Int(height)) が現れました (pid=\(pid), 起動から \(elapsedMs) ms)"
            break
        }
    }
    if windowLine != nil { break }
    Thread.sleep(forTimeInterval: 0.1)
}
guard let observed = windowLine else {
    stop()
    fail("\(Int(timeoutSeconds)) 秒以内にウィンドウが現れませんでした")
}
print(observed)

// 2. 初回描画（8.2）の成立行と、そこに載る**描画された画面**（10.4）を待つ。8.2 の
//    期限はウィンドウ生成から 3 秒なので、これを大きく超える猶予で足りる。
let renderDeadline = Date().addingTimeInterval(20.0)
var heartbeat: String? = nil
var actual: String? = nil
while Date() < renderDeadline {
    for line in recordText().split(separator: "\n") {
        let text = String(line)
        if heartbeatPatterns.contains(where: { text.contains($0) }) {
            heartbeat = text
            actual = renderedScreen(in: text)
        }
    }
    if heartbeat != nil {
        // 期待が空なら成立だけでよい。期待があるなら**描画された画面**が一致する
        // ことを要求する（通知はウィンドウごとに 1 回なので、待っても変わらない）。
        if expectedScreen.isEmpty { break }
        if actual == expectedScreen { break }
        stop()
        let actualText = actual ?? "（報告なし）"
        fail("描画は成立しましたが、描画された画面が期待と一致しません: 期待 screen=\(expectedScreen) / 実際 screen=\(actualText)（起動時の指定が登録簿に無いか、描画が既定の画面へ落ちています）")
    }
    if !process.isRunning { break }
    Thread.sleep(forTimeInterval: 0.2)
}
guard let painted = heartbeat else {
    stop()
    fail("初回描画の成立行（'\(heartbeatPatterns[0])'）が 20 秒以内に現れませんでした（初回描画が成立していない）")
}
print("初回描画: \(painted)")
if !expectedScreen.isEmpty {
    let actualText = actual ?? "（報告なし）"
    print("描画された画面: \(actualText)（期待 \(expectedScreen)）")
    // 要求の行は**証明ではなく起動の識別**として出す（あれば。配布物は出さない）。
    for line in recordText().split(separator: "\n") where String(line).contains(requestedPattern) {
        print("初期画面の指定: \(line)")
    }
}
stop()
exit(0)
