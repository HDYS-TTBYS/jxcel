import ApplicationServices
import CoreGraphics
import Darwin
import Foundation

// 単一インスタンス化と複数ウィンドウの 3 経路を macOS 上で検査する（tasks.md 10.5）。
// 引数: <配布物> <検証用の形> <記録ファイル> <アプリ出力の接頭辞> <ドキュメント位置>
//       <題名> <タイムアウト秒>
//
// ウィンドウの観測は CoreGraphics（所有者 pid・レイヤー・寸法のみ。10.4 と同じ）。
// **2 つのプロセスを同時に動かすので、数えるのは 1 つ目の pid のウィンドウだけ**である。
let distApp = CommandLine.arguments[1]
let verifyApp = CommandLine.arguments[2]
let recordPath = CommandLine.arguments[3]
let outputPrefix = CommandLine.arguments[4]
let documentPath = CommandLine.arguments[5]
let title = CommandLine.arguments[6]
let timeoutSeconds = Double(CommandLine.arguments[7]) ?? 60.0

var running: [Process] = []
var outputCounter = 0

func note(_ message: String) {
    print(message)
    fflush(stdout)
}

func recordLines() -> [String] {
    // **末尾の空要素を数に入れない。** `omittingEmptySubsequences: false` は末尾の改行の
    // 後ろにも空の要素を作るので、その `count` を**位置**として使うと（下の
    // `phase… = recordLines().count`）**次の 1 行を飛ばす**。実測（2026-09-12 の macOS の
    // ランナー）: 3 つ目の起動（引数なし）の後に書かれた最初の行がまさに
    // `二重起動を引き継ぎました … ドキュメント要求なし` だったため、10 秒待っても
    // （30 秒に伸ばしても）見つからず、失敗時のダンプ（`.suffix(30)`）には現れていた
    // — 走査の窓だけが 1 行ずれていた。**行の位置は「改行で区切られた行の数」で数える。**
    let text = (try? String(contentsOfFile: recordPath, encoding: .utf8)) ?? ""
    var lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
    while lines.last == "" {
        lines.removeLast()
    }
    return lines
}

// 記録の `<offset>` 行目より後から正規表現に一致する最初の行を待つ。
func findLine(_ pattern: String, from offset: Int, seconds: Double) -> String? {
    let deadline = Date().addingTimeInterval(seconds)
    while true {
        let lines = recordLines()
        if lines.count > offset {
            for line in lines[offset...] where line.range(of: pattern, options: .regularExpression) != nil {
                return line
            }
        }
        if Date() >= deadline { return nil }
        Thread.sleep(forTimeInterval: 0.2)
    }
}

func countLines(_ pattern: String, from offset: Int) -> Int {
    let lines = recordLines()
    guard lines.count > offset else { return 0 }
    return lines[offset...].filter { $0.range(of: pattern, options: .regularExpression) != nil }.count
}

/// その pid が所有する**画面に出ている**ウィンドウの識別子の集合（`kCGWindowNumber`）。
///
/// 数ではなく集合を使うのは、**入れ替わり**（1 枚消えて別の 1 枚が現れる）も
/// 数え落とさないためである。フィルタは「通常レイヤー（0）・100x100 以上・
/// **`kCGWindowIsOnscreen` が真・`kCGWindowAlpha` が正**」であり、**利用者に見えている
/// トップレベルウィンドウ**だけを数える。
///
/// **「画面に出ている」を条件に加えた理由**（実測: 2026-09-12 の macOS のランナー）:
/// `.optionAll` の一覧にはプロセスが持つ**見えないウィンドウ**も含まれ、レイヤーと
/// 大きさだけのフィルタではそれも数えてしまう。実際、ウィンドウを 1 枚も開いていない
/// 状態（閉じた後）でも一覧に残る 1 枚があり、`0 枚になるのを待つ` 検査が偽の失敗に
/// なった。1.5 / 10.4 の段は「1 枚以上ある」ことしか見ないので影響を受けていない。
func windowIds(of pid: pid_t) -> Set<Int> {
    guard let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else {
        return []
    }
    var ids = Set<Int>()
    for window in list {
        guard let owner = window[kCGWindowOwnerPID as String] as? Int, owner == Int(pid) else { continue }
        guard let layer = window[kCGWindowLayer as String] as? Int, layer == 0 else { continue }
        guard let onscreen = window[kCGWindowIsOnscreen as String] as? Bool, onscreen else { continue }
        guard let alpha = window[kCGWindowAlpha as String] as? Double, alpha > 0 else { continue }
        guard let bounds = window[kCGWindowBounds as String] as? [String: Any],
              let width = bounds["Width"] as? Double, let height = bounds["Height"] as? Double,
              width >= 100, height >= 100 else { continue }
        guard let number = window[kCGWindowNumber as String] as? Int else { continue }
        ids.insert(number)
    }
    return ids
}

func windowCount(of pid: pid_t) -> Int {
    return windowIds(of: pid).count
}

/// 集合を読みやすい形にする（失敗の診断に使う）。
func describe(_ ids: Set<Int>) -> String {
    return "[" + ids.sorted().map(String.init).joined(separator: ", ") + "]"
}

/// 集合を `samples` 回サンプリングする（`interval` 秒ごと）。**過渡的な増減を人の目で
/// 追えるようにする**ためであり、判定には使わない。
func sampleWindowSet(of pid: pid_t, samples: Int, interval: Double) -> [Set<Int>] {
    var result: [Set<Int>] = []
    for _ in 0..<samples {
        result.append(windowIds(of: pid))
        Thread.sleep(forTimeInterval: interval)
    }
    return result
}

/// その pid のすべてのウィンドウを「id / レイヤー / 大きさ / 画面内か」で並べる。
///
/// **目的は増えた 1 枚の正体を残すこと。** ウィンドウサーバの一覧には、アプリが作った
/// トップレベルでないもの（フレームワークが持つ隠しウィンドウなど）が混ざりうるので、
/// 「増えた」と言うだけでは原因が分からない（実測: 2026-09-12 の macOS のランナーで
/// 集合が [63, 68] → [63, 68, 69] になった）。
func describeWindows(of pid: pid_t) -> String {
    guard let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else {
        return "(ウィンドウの一覧を取得できません)"
    }
    var parts: [String] = []
    for window in list {
        guard let owner = window[kCGWindowOwnerPID as String] as? Int, owner == Int(pid) else { continue }
        let number = window[kCGWindowNumber as String] as? Int ?? -1
        let layer = window[kCGWindowLayer as String] as? Int ?? -1
        let onscreen = window[kCGWindowIsOnscreen as String] as? Bool ?? false
        let bounds = window[kCGWindowBounds as String] as? [String: Any]
        let width = bounds?["Width"] as? Double ?? -1
        let height = bounds?["Height"] as? Double ?? -1
        // `kCGWindowOwnerName` はアプリ名、`kCGWindowName` は**画面収録の許可が無いと
        // `nil`** になりうる（どちらも診断のためだけに読む）。
        let ownerName = window[kCGWindowOwnerName as String] as? String ?? "(名前なし)"
        let windowName = window[kCGWindowName as String] as? String ?? "(名前なし)"
        parts.append("#\(number) layer=\(layer) \(Int(width))x\(Int(height)) onscreen=\(onscreen) owner=\(ownerName) name=\(windowName)")
    }
    return parts.isEmpty ? "(0 枚)" : parts.joined(separator: " / ")
}

/// ウィンドウの集合を `seconds` の間観測し、**最後の 2 標本が一致し、かつ `expected` と
/// 一致する**かどうかを返す（一致しなければ最後の標本を返す）。
///
/// **過渡的な増減を許し、持続する差だけを失敗にする。** macOS のウィンドウサーバは
/// ウィンドウの**作り直し**（前面化・非表示化の遷移）の途中で集合を一時的に変えうるので、
/// 1 標本だけの比較は偽の失敗になる（Linux の検査器が `stable_wait` で「2 回続けて同じ
/// 集合」を要求しているのと同じ判断）。**新しく現れたウィンドウが残れば失敗する**
/// （集合が `expected` に戻らない）ので、主張は弱まらない。
func settleWindowSet(of pid: pid_t, expected: Set<Int>, seconds: Double) -> (Set<Int>, Bool) {
    let deadline = Date().addingTimeInterval(seconds)
    while Date() < deadline {
        Thread.sleep(forTimeInterval: 0.2)
    }
    let first = windowIds(of: pid)
    Thread.sleep(forTimeInterval: 0.2)
    let second = windowIds(of: pid)
    return (second, first == expected && second == expected)
}

func waitForWindowCount(of pid: pid_t, atLeast expected: Int, seconds: Double) -> Int? {
    let deadline = Date().addingTimeInterval(seconds)
    while true {
        let count = windowCount(of: pid)
        if count >= expected { return count }
        if Date() >= deadline { return nil }
        Thread.sleep(forTimeInterval: 0.1)
    }
}

func waitForWindowsGone(of pid: pid_t, seconds: Double) -> Bool {
    let deadline = Date().addingTimeInterval(seconds)
    while true {
        if windowCount(of: pid) == 0 { return true }
        if Date() >= deadline { return false }
        Thread.sleep(forTimeInterval: 0.1)
    }
}

func stopAll() {
    for process in running where process.isRunning {
        process.terminate()
    }
    Thread.sleep(forTimeInterval: 0.5)
    for process in running where process.isRunning {
        kill(process.processIdentifier, SIGKILL)
        process.waitUntilExit()
    }
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write("NG: \(message)\n".data(using: .utf8)!)
    FileHandle.standardError.write("--- 診断記録（末尾）: \(recordPath) ---\n".data(using: .utf8)!)
    for line in recordLines().suffix(30) {
        FileHandle.standardError.write("\(line)\n".data(using: .utf8)!)
    }
    stopAll()
    exit(1)
}

// アプリを起動する。検証専用の環境変数は親から漏らさず、`denyClose` のときだけ
// `JXCEL_VERIFICATION_DENY_CLOSE` を、`exitAfterMs` のときだけ
// `JXCEL_VERIFICATION_EXIT_AFTER_MS` を渡す（配布物はどちらも読まない）。
func launch(_ executable: String, arguments: [String], denyClose: String?, exitAfterMs: String? = nil) -> Process {
    outputCounter += 1
    let outputPath = "\(outputPrefix)-\(outputCounter).log"
    _ = FileManager.default.createFile(atPath: outputPath, contents: nil)
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    var environment = ProcessInfo.processInfo.environment
    environment.removeValue(forKey: "JXCEL_VERIFICATION_DENY_CLOSE")
    environment.removeValue(forKey: "JXCEL_VERIFICATION_INITIAL_SCREEN")
    environment.removeValue(forKey: "JXCEL_VERIFICATION_EXIT_AFTER_MS")
    if let denyClose = denyClose {
        environment["JXCEL_VERIFICATION_DENY_CLOSE"] = denyClose
    }
    if let exitAfterMs = exitAfterMs {
        environment["JXCEL_VERIFICATION_EXIT_AFTER_MS"] = exitAfterMs
    }
    process.environment = environment
    if let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: outputPath)) {
        process.standardOutput = handle
        process.standardError = handle
    }
    do {
        try process.run()
    } catch {
        fail("アプリを起動できません: \(executable): \(error)")
    }
    running.append(process)
    return process
}

func waitForExit(_ process: Process, seconds: Double) -> Bool {
    let deadline = Date().addingTimeInterval(seconds)
    while process.isRunning {
        if Date() >= deadline { return false }
        Thread.sleep(forTimeInterval: 0.1)
    }
    return true
}

func stop(_ process: Process) {
    if process.isRunning { process.terminate() }
    let deadline = Date().addingTimeInterval(10)
    while process.isRunning && Date() < deadline { Thread.sleep(forTimeInterval: 0.1) }
    if process.isRunning { kill(process.processIdentifier, SIGKILL) }
    let reapDeadline = Date().addingTimeInterval(5)
    while process.isRunning && Date() < reapDeadline { Thread.sleep(forTimeInterval: 0.1) }
    process.waitUntilExit()
}

// アクセシビリティ API で「閉じる」ボタンを押す（許可が無ければ false）。
func pressCloseButton(of pid: pid_t) -> Bool {
    guard AXIsProcessTrusted() else { return false }
    let application = AXUIElementCreateApplication(pid)
    var windowsValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(application, kAXWindowsAttribute as CFString, &windowsValue) == .success,
          let windows = windowsValue as? [AXUIElement], let window = windows.first else { return false }
    var buttonValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(window, kAXCloseButtonAttribute as CFString, &buttonValue) == .success,
          let button = buttonValue, CFGetTypeID(button) == AXUIElementGetTypeID() else { return false }
    return AXUIElementPerformAction(button as! AXUIElement, kAXPressAction as CFString) == .success
}

// **記録は消さない**（起動中のアプリが書き続ける）。各段の開始時の行数から後だけを調べる。
_ = FileManager.default.createFile(atPath: recordPath, contents: nil)

note("検証 (a)(b): 1 つ目として配布物を起動する（引数なし → ドキュメントを関連付けないウィンドウ）")
let phaseAB = recordLines().count
let first = launch(distApp, arguments: [], denyClose: nil)
guard let firstWindows = waitForWindowCount(of: first.processIdentifier, atLeast: 1, seconds: timeoutSeconds) else {
    fail("1 つ目のウィンドウが \(Int(timeoutSeconds)) 秒以内に現れませんでした")
}
guard findLine("ウィンドウを開いた: label=empty-1 ", from: phaseAB, seconds: 10) != nil else {
    fail("1 つ目のウィンドウが label=empty-1 として記録に現れません")
}
note("検証 (a): 1 つ目の起動: pid=\(first.processIdentifier) ウィンドウ数=\(firstWindows)")

note("検証 (a)(b): 同じ配布物を再度実行する（ドキュメント位置を渡す → 単一インスタンスが引数を引き渡す）")
let second = launch(distApp, arguments: [documentPath], denyClose: nil)
if !waitForExit(second, seconds: timeoutSeconds) {
    stop(second)
    fail("2 つ目の起動が \(Int(timeoutSeconds)) 秒以内に終了しません（常駐している。単一インスタンスが成立していない）")
}
if second.terminationStatus != 0 {
    fail("2 つ目の起動が終了コード \(second.terminationStatus) で終わった（0 であるべき）")
}
note("検証 (a): 2 つ目の起動は終了コード 0 で終わった（常駐しない）")
if !first.isRunning {
    fail("2 つ目の起動の後に 1 つ目のプロセスが終了した")
}
guard let secondWindows = waitForWindowCount(of: first.processIdentifier, atLeast: firstWindows + 1, seconds: timeoutSeconds) else {
    fail("2 つ目の起動の後、ウィンドウ数が \(firstWindows + 1) 以上になりませんでした（引き継いだ側がウィンドウを提示していない）")
}
guard let handover = findLine("二重起動を引き継ぎました.*ドキュメント要求 ", from: phaseAB, seconds: 10) else {
    fail("引き継ぎの行（二重起動を引き継ぎました … ドキュメント要求 …）が記録に現れません")
}
guard let docLabelLine = findLine("ウィンドウを開いた: label=doc-", from: phaseAB, seconds: 10) else {
    fail("新しいウィンドウが label=doc-*（ドキュメント付き）として記録に現れません")
}
let docLabel = docLabelLine.components(separatedBy: "label=").last?.components(separatedBy: " ").first ?? "(不明)"
note("検証 (b): ウィンドウ数=\(secondWindows)（1 つ目=\(firstWindows) から 1 増えた。同じ pid のウィンドウを数えている）")
note("検証 (b): 新しいウィンドウのラベル=\(docLabel)（doc-* ＝ ドキュメント付き。ラベルは記録が出す）")
// **その時点のウィンドウの一覧を残す**（増減があったとき、何が増えたのかを事後に分かる
// ようにする。実測: 2026-09-12 の macOS のランナーで集合が [62, 67] → [62, 67, 68] と
// 増えた）。
note("検証 (b): 1 つ目のプロセスのウィンドウ: \(describeWindows(of: first.processIdentifier))")
note("検証 (b): 記録（引き継ぎ）: \(handover)")

note("検証 (a): 引数なしで再度実行する（既にあるウィンドウを前面に出すだけで、新しいウィンドウを作らない）")
let phaseArgless = recordLines().count
// **起動の直前の集合**を取る（この段の直前の観測は揺れうるので、ここで取り直す）。
// 0.5 秒ごとに 5 標本を取り、**揺れていないこと**と**増えた 1 枚の正体**を残す。
let beforeSamples = sampleWindowSet(of: first.processIdentifier, samples: 5, interval: 0.5)
let setBeforeArgless = beforeSamples.last ?? []
note("検証 (a): 引数なしの起動の前の集合（0.5 秒ごと 5 標本）: \(beforeSamples.map(describe).joined(separator: " → "))")
let third = launch(distApp, arguments: [], denyClose: nil)
if !waitForExit(third, seconds: timeoutSeconds) {
    stop(third)
    fail("引数なしの 2 つ目の起動が \(Int(timeoutSeconds)) 秒以内に終了しません")
}
if third.terminationStatus != 0 {
    fail("引数なしの 2 つ目の起動が終了コード \(third.terminationStatus) で終わった（0 であるべき）")
}
guard let argless = findLine("二重起動を引き継ぎました.*ドキュメント要求なし", from: phaseArgless, seconds: 10) else {
    fail("引数なしの引き継ぎの行（… ドキュメント要求なし）が記録に現れません")
}
// **過渡的な増減を許し、持続する差だけを失敗にする**（集合で見るので入れ替わりも解る）。
let afterSamples = sampleWindowSet(of: first.processIdentifier, samples: 5, interval: 0.5)
let (setAfterArgless, settled) = settleWindowSet(of: first.processIdentifier, expected: setBeforeArgless, seconds: 5)
if !settled {
    fail("引数なしの 2 つ目の起動の後、ウィンドウの集合が \(describe(setBeforeArgless)) に落ち着きませんでした（起動後 0.5 秒ごとの 5 標本: \(afterSamples.map(describe).joined(separator: " → ")) / 落ち着き待ちの最後の観測 \(describe(setAfterArgless)) / その時点の全ウィンドウ: \(describeWindows(of: first.processIdentifier))）— 新しいウィンドウを作ってはならない")
}
note("検証 (a): 引数なしの 2 つ目の起動も終了コード 0 で終わり、ウィンドウの集合=\(describe(setAfterArgless))（起動前と同じ \(secondWindows) 枚）のまま変わらない")
note("検証 (a): 引数なしの起動の後のウィンドウ: \(describeWindows(of: first.processIdentifier))")
note("検証 (a): 記録（引数なしの引き継ぎ）: \(argless)")

note("検証: 配布物のインスタンスを片付ける")
let firstPid = first.processIdentifier
stop(first)
if !waitForWindowsGone(of: firstPid, seconds: timeoutSeconds) {
    fail("配布物のウィンドウが \(Int(timeoutSeconds)) 秒以内に消えませんでした")
}

let phaseDeny = recordLines().count
note("検証 (c): 検証用の形を起動する（JXCEL_VERIFICATION_DENY_CLOSE=doc-1 → doc-1 の終了を拒否する委譲先）")
let denied = launch(verifyApp, arguments: [documentPath], denyClose: "doc-1")
guard let deniedWindows = waitForWindowCount(of: denied.processIdentifier, atLeast: 1, seconds: timeoutSeconds) else {
    fail("検証用の形のウィンドウが \(Int(timeoutSeconds)) 秒以内に現れませんでした")
}
guard findLine("ウィンドウを開いた: label=doc-1 ", from: phaseDeny, seconds: 10) != nil else {
    fail("検証用の形が doc-1 というラベルのウィンドウを開いていません（拒否の対象が存在しない）")
}
guard findLine("初回描画が成立した: label=doc-1 ", from: phaseDeny, seconds: 20) != nil
      || findLine("初回描画は成立したがソフトウェアラスタライザ経由である: label=doc-1 ", from: phaseDeny, seconds: 1) != nil
      || findLine("期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label=doc-1 ", from: phaseDeny, seconds: 1) != nil else {
    fail("初回描画の成立行が現れません（購読が張られたことを確認できない）")
}
note("検証 (c): ウィンドウ数=\(deniedWindows)")

if AXIsProcessTrusted() {
    note("検証 (c): アクセシビリティ許可あり — 閉じるボタンで閉鎖要求を注入する")
    if pressCloseButton(of: denied.processIdentifier) {
        guard let denyLine = findLine("can_close_window: 呼び出し元ウィンドウ = doc-1 / 判定 = 拒否", from: phaseDeny, seconds: 10) else {
            fail("拒否の往復（can_close_window の判定 = 拒否）が記録に現れません")
        }
        let settleDeny = Date().addingTimeInterval(3)
        while Date() < settleDeny {
            if windowCount(of: denied.processIdentifier) < deniedWindows {
                fail("拒否されたはずのウィンドウが閉じた（委譲先の拒否が効いていない）")
            }
            if !denied.isRunning {
                fail("拒否された後にプロセスが終了した")
            }
            Thread.sleep(forTimeInterval: 0.2)
        }
        let roundTrips = countLines("can_close_window: 呼び出し元ウィンドウ = doc-1 ", from: phaseDeny)
        if roundTrips != 1 {
            fail("1 回の終了要求に対する拒否の往復が \(roundTrips) 回だった（1 回であるべき。7.6 の実測）")
        }
        note("検証 (c): 拒否 — ウィンドウ数=\(deniedWindows) のまま、プロセス pid=\(denied.processIdentifier) も生存")
        note("検証 (c): 記録（拒否の往復）: \(denyLine)")
        note("検証 (c): 拒否の往復の回数=\(roundTrips)（1 回の終了要求につき 1 回）")

        stop(denied)
        let phaseAllow = recordLines().count
        note("検証 (c-2): 対照 — 検証用の形を起動する（JXCEL_VERIFICATION_DENY_CLOSE なし → 常に許可する委譲先）")
        let allowed = launch(verifyApp, arguments: [documentPath], denyClose: nil)
        guard waitForWindowCount(of: allowed.processIdentifier, atLeast: 1, seconds: timeoutSeconds) != nil else {
            fail("対照のウィンドウが \(Int(timeoutSeconds)) 秒以内に現れませんでした")
        }
        guard findLine("初回描画が成立した: label=doc-1 ", from: phaseAllow, seconds: 20) != nil
              || findLine("初回描画は成立したがソフトウェアラスタライザ経由である: label=doc-1 ", from: phaseAllow, seconds: 1) != nil
              || findLine("期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label=doc-1 ", from: phaseAllow, seconds: 1) != nil else {
            fail("対照の初回描画の成立行が現れません")
        }
        if pressCloseButton(of: allowed.processIdentifier) {
            if !waitForWindowsGone(of: allowed.processIdentifier, seconds: timeoutSeconds) {
                fail("拒否しない委譲先で閉鎖要求を送ったが、ウィンドウが消えませんでした（許可が効いていない）")
            }
            note("検証 (c-2): ウィンドウは消えました")
            // **macOS の慣習に合わせる（要件 2.9。`Residency::CURRENT` = StayResident）。**
            // 最後のウィンドウを閉じてもプロセスは**終了しない**。ここで「終了する」ことを
            // 期待してはならない — 期待すると、プラットフォームの慣習どおりの常駐を
            // 「欠陥」として報告してしまう（旧い形がそうなっていた）。**要件 2.8
            // （最後のウィンドウが閉じたら終了する）は Linux と Windows の段が実測する** —
            // `vetoes_exit` が拒否するのは macOS の `code: None` だけである。
            let settleResident = Date().addingTimeInterval(3)
            while Date() < settleResident {
                if !allowed.isRunning {
                    fail("最後のウィンドウを閉じた後にプロセスが終了しました（macOS は常駐する。要件 2.9）")
                }
                Thread.sleep(forTimeInterval: 0.2)
            }
            note("検証 (c-2): 最後のウィンドウを閉じてもプロセス pid=\(allowed.processIdentifier) は常駐している（要件 2.9）")
            stop(allowed)
        } else {
            note("検証 (c-2): 対照の閉じるボタンを押せなかった（注入できず）")
            stop(allowed)
        }
    } else {
        note("検証 (c): アクセシビリティ許可はあるが、閉じるボタンを押せなかった（注入できず）")
        stop(denied)
    }
} else {
    note("macOS: アクセシビリティ許可が無いため、外部からウィンドウの閉鎖要求を注入できない（GitHub の macOS ランナーは許可を与えない。actions/runner-images#8214）。**経路 (c) の拒否と許可の実測は Linux と Windows の段が担う** — この段は (a)/(b) と、検証用の形が doc-1 を開くことまでを実測した。許可が与えられた環境ではこの分岐が閉じるボタンの押下で同じ検査を行う。")
    stop(denied)
}

// 検証 (d): **明示的な終了は常駐の慣習でもプロセスを終わらせる**（要件 2.9、5.4 の
// 完了状態）。検証用の引き金 `<ミリ秒>` は 5.4 の明示的な終了（`request_exit`）そのもので、
// メニューの「終了」項目と終了コマンドが呼ぶ関数と同じである。**アクセシビリティ許可を
// 要さない**ので、許可の無いランナーでもこの検査は成立する（(c) と違い注入が要らない）。
// 待ちは 6 秒にしてある — ウィンドウの出現（起動予算 2 秒。要件 1.3）より確実に後に
// 終了させるためである（2 秒だと出現と終了が競合しうる）。
note("検証 (d): 検証用の形を起動し、明示的な終了（JXCEL_VERIFICATION_EXIT_AFTER_MS=6000 → request_exit）でプロセスが終わることを確かめる")
let quitting = launch(verifyApp, arguments: [], denyClose: nil, exitAfterMs: "6000")
guard waitForWindowCount(of: quitting.processIdentifier, atLeast: 1, seconds: timeoutSeconds) != nil else {
    fail("明示的な終了の検証でウィンドウが \(Int(timeoutSeconds)) 秒以内に現れませんでした")
}
if !waitForExit(quitting, seconds: timeoutSeconds) {
    stop(quitting)
    fail("明示的な終了（request_exit）の後にプロセスが終了しませんでした（常駐の慣習でも明示的な終了は効かなければならない。要件 2.9 / 5.4）")
}
if quitting.terminationStatus != 0 {
    fail("明示的な終了が終了コード \(quitting.terminationStatus) で終わった（通常終了は 0）")
}
note("検証 (d): 明示的な終了でプロセスは終了コード 0 で終わった（常駐の慣習でも明示的な終了は確実に効く）")

stopAll()
note("検証 (後始末): 起動したプロセスをすべて終了した")
exit(0)
