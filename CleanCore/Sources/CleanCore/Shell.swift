import Foundation


/// Minimal async process runner.
public enum Shell {
    public struct Output: Sendable {
        public let stdout: String
        public let stderr: String
        public let status: Int32
        public var ok: Bool { status == 0 }
    }

    public static func run(_ executable: String, _ args: [String], timeout: TimeInterval = 60) async -> Output {
        await withCheckedContinuation { cont in
            let task = Process()
            task.executableURL = URL(filePath: executable)
            task.arguments = args
            var env = ProcessInfo.processInfo.environment
            // Fixed folders first, then only the absolute parts of the
            // inherited PATH (never "." or an empty entry).
            let inherited = (env["PATH"] ?? "").split(separator: ":").filter { $0.hasPrefix("/") }
            env["PATH"] = (["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"] + inherited.map(String.init))
                .joined(separator: ":")
            task.environment = env
            task.standardInput = FileHandle.nullDevice
            let outPipe = Pipe(), errPipe = Pipe()
            task.standardOutput = outPipe
            task.standardError = errPipe
            do {
                try task.run()
            } catch {
                cont.resume(returning: Output(stdout: "", stderr: error.localizedDescription, status: -1))
                return
            }
            let timer = DispatchWorkItem { if task.isRunning { task.terminate() } }
            DispatchQueue.global().asyncAfter(deadline: .now() + timeout, execute: timer)
            // Both pipes are drained at once on GCD threads, never on Swift's
            // concurrency threads (blocking those can starve the app), and a
            // tool that fills stderr cannot stall waiting for stdout to be read.
            let box = Collected()
            let group = DispatchGroup()
            DispatchQueue.global().async(group: group) { box.out = outPipe.fileHandleForReading.readDataToEndOfFile() }
            DispatchQueue.global().async(group: group) { box.err = errPipe.fileHandleForReading.readDataToEndOfFile() }
            group.notify(queue: .global()) {
                task.waitUntilExit()
                timer.cancel()
                cont.resume(returning: Output(
                    stdout: String(decoding: box.out, as: UTF8.self),
                    stderr: String(decoding: box.err, as: UTF8.self),
                    status: task.terminationStatus
                ))
            }
        }
    }

    /// Output gathered by the two reader threads; each writes only its own
    /// field and both finish before the group notifies.
    private final class Collected: @unchecked Sendable {
        var out = Data()
        var err = Data()
    }

    public static func which(_ name: String) -> String? {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin"] {
            let p = "\(dir)/\(name)"
            if FileManager.default.isExecutableFile(atPath: p) { return p }
        }
        return nil
    }
}
