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
            env["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:" + (env["PATH"] ?? "")
            task.environment = env
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
            let outData = outPipe.fileHandleForReading.readDataToEndOfFile()
            let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
            task.waitUntilExit()
            timer.cancel()
            cont.resume(returning: Output(
                stdout: String(decoding: outData, as: UTF8.self),
                stderr: String(decoding: errData, as: UTF8.self),
                status: task.terminationStatus
            ))
        }
    }

    public static func which(_ name: String) -> String? {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin"] {
            let p = "\(dir)/\(name)"
            if FileManager.default.isExecutableFile(atPath: p) { return p }
        }
        return nil
    }
}
