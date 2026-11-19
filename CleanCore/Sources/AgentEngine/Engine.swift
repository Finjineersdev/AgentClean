// SPDX-License-Identifier: Apache-2.0
//
// The app's only way to change files: it runs the bundled `agentclean`
// binary (ADR 0005). Arguments go straight to the process, never through a
// shell. Changing commands are dry runs unless `execute` is true, exactly as
// on the command line.

import Foundation

public enum EngineError: Error, Equatable, Sendable {
    /// The engine reported an error (`{"error": …}`).
    case failed(String)
    /// The engine did not print what the contract says.
    case badOutput(String)
    /// The binary could not be started.
    case launch(String)
}

public struct Engine: Sendable {
    public let binary: URL
    /// Quarantine folder; nil uses the engine's default.
    public let store: URL?
    /// Extra environment, for tests (HOME, XDG_*).
    public let environment: [String: String]?

    public init(binary: URL, store: URL? = nil, environment: [String: String]? = nil) {
        self.binary = binary
        self.store = store
        self.environment = environment
    }

    // MARK: Commands

    public func plan(project: URL) async throws -> PlanOutput {
        try await call(["plan", project.path])
    }

    /// A dry run unless `execute`. `only` narrows to exact folders.
    public func clean(
        projects: [URL],
        only: [URL] = [],
        includeCaution: Bool = false,
        includeActive: Bool = false,
        execute: Bool = false
    ) async throws -> CleanOutput {
        var args = ["clean"] + storeArgs
        if includeCaution { args.append("--include-caution") }
        if includeActive { args.append("--include-active") }
        if execute { args.append("--execute") }
        for folder in only { args += ["--only", folder.path] }
        args.append("--")
        args += projects.map(\.path)
        return try await call(args)
    }

    public func history() async throws -> [Run] {
        let out: HistoryOutput = try await call(["history"] + storeArgs)
        return out.runs
    }

    public func restore(run: String, item: Int? = nil) async throws -> Run {
        var args = ["restore"] + storeArgs
        if let item { args += ["--item", String(item)] }
        args += ["--", run]
        let out: RestoreOutput = try await call(args)
        return out.run
    }

    /// A dry run unless `execute`.
    public func purge(run: String, execute: Bool = false) async throws -> PurgeOutput {
        var args = ["purge"] + storeArgs
        if execute { args.append("--execute") }
        args += ["--", run]
        return try await call(args)
    }

    public func pins() async throws -> [String] {
        let out: PinsOutput = try await call(["pin"])
        return out.pinned
    }

    public func pin(_ folder: URL) async throws -> [String] {
        let out: PinsOutput = try await call(["pin", "--", folder.path])
        return out.pinned
    }

    public func unpin(_ folder: URL) async throws -> [String] {
        let out: PinsOutput = try await call(["unpin", "--", folder.path])
        return out.pinned
    }

    /// Projects as they are found, then skipped folders and a summary last.
    /// Cancelling the consuming task stops the engine.
    public func scan(roots: [URL]) -> AsyncThrowingStream<ScanEvent, Error> {
        let args = ["--json", "scan"] + storeArgs + ["--"] + roots.map(\.path)
        return AsyncThrowingStream { continuation in
            let process: Process
            let exit: AsyncStream<Int32>
            let out = Pipe()
            do {
                (process, exit) = try launch(args, stdout: out)
            } catch {
                continuation.finish(throwing: error)
                return
            }
            let reader = Task.detached {
                do {
                    for try await line in out.fileHandleForReading.bytes.lines {
                        let data = Data(line.utf8)
                        if let failure = try? JSONDecoder().decode(ErrorOutput.self, from: data) {
                            throw EngineError.failed(failure.error)
                        }
                        continuation.yield(try Self.decode(ScanEvent.self, from: data))
                    }
                    let status = await Self.status(exit)
                    if status != 0 {
                        throw EngineError.badOutput("scan exited with \(status)")
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in
                if process.isRunning { process.terminate() }
                reader.cancel()
            }
        }
    }

    // MARK: Plumbing

    private var storeArgs: [String] {
        store.map { ["--store", $0.path] } ?? []
    }

    static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        do {
            return try decoder.decode(type, from: data)
        } catch {
            throw EngineError.badOutput("\(error)")
        }
    }

    /// Runs one command and decodes its single JSON document. Exit code 3
    /// (some items failed) still carries a document saying which.
    private func call<T: Decodable>(_ args: [String]) async throws -> T {
        let out = Pipe()
        let err = Pipe()
        let (process, exit) = try launch(["--json"] + args, stdout: out, stderr: err)
        let (stdout, stderr) = await withTaskCancellationHandler {
            async let o = Self.readAll(out)
            async let e = Self.readAll(err)
            return await (o, e)
        } onCancel: {
            if process.isRunning { process.terminate() }
        }
        let status = await Self.status(exit)
        if let failure = try? JSONDecoder().decode(ErrorOutput.self, from: stdout) {
            throw EngineError.failed(failure.error)
        }
        guard [0, 3].contains(status) else {
            let message = String(decoding: stderr, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw EngineError.failed(message.isEmpty ? "exit \(status)" : message)
        }
        return try Self.decode(T.self, from: stdout)
    }

    /// Starts the engine. The exit status arrives on the stream; it is set up
    /// before launch so a fast exit cannot be missed (`waitUntilExit` needs a
    /// run loop and hangs on Swift's concurrency threads).
    private func launch(
        _ args: [String], stdout: Pipe, stderr: Pipe? = nil
    ) throws -> (Process, AsyncStream<Int32>) {
        let (exit, done) = AsyncStream<Int32>.makeStream()
        let process = Process()
        process.terminationHandler = { p in
            done.yield(p.terminationStatus)
            done.finish()
        }
        process.executableURL = binary
        process.arguments = args
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = stdout
        process.standardError = stderr ?? FileHandle.nullDevice
        if let environment {
            process.environment = ProcessInfo.processInfo.environment
                .merging(environment) { _, new in new }
        }
        do {
            try process.run()
        } catch {
            throw EngineError.launch("\(binary.path): \(error.localizedDescription)")
        }
        return (process, exit)
    }

    private static func status(_ exit: AsyncStream<Int32>) async -> Int32 {
        for await status in exit { return status }
        return -1
    }

    private static func readAll(_ pipe: Pipe) async -> Data {
        await Task.detached {
            (try? pipe.fileHandleForReading.readToEnd()) ?? Data()
        }.value
    }
}
