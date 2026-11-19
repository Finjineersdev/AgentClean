import Foundation

/// Talks to the `docker` CLI. Images, volumes, and containers are mapped to
/// compose projects via labels so anything belonging to an active or pinned
/// project is never offered.
public actor DockerService {
    public static let shared = DockerService()
    public nonisolated let scheme = "docker"

    public struct Image: Sendable, Identifiable {
        public var id: String { imageID }
        public let imageID: String
        public let repository: String
        public let tag: String
        public let sizeBytes: UInt64
        public let uniqueBytes: UInt64
        public let containers: Int
        public let created: String
        public var ref: String { tag == "<none>" ? imageID : "\(repository):\(tag)" }
        public var shortID: String { String(imageID.replacingOccurrences(of: "sha256:", with: "").prefix(12)) }
    }

    public struct Volume: Sendable, Identifiable {
        public var id: String { name }
        public let name: String
        public let sizeBytes: UInt64
        public let links: Int
        public let composeProject: String?
    }

    public struct Container: Sendable, Identifiable {
        public var id: String { containerID }
        public let containerID: String
        public let name: String
        public let image: String
        public let state: String
        public let composeProject: String?
        public let workingDir: String?
        public let sizeBytes: UInt64
        public var isRunning: Bool { state.lowercased() == "running" }
    }

    public struct Snapshot: Sendable {
        public let available: Bool
        public let images: [Image]
        public let volumes: [Volume]
        public let containers: [Container]
        public let buildCacheBytes: UInt64
        public static let unavailable = Snapshot(available: false, images: [], volumes: [], containers: [], buildCacheBytes: 0)
    }

    private var dockerPath: String? { Shell.which("docker") }

    public func isAvailable() async -> Bool {
        guard let docker = dockerPath else { return false }
        return await Shell.run(docker, ["info", "--format", "{{.ServerVersion}}"], timeout: 10).ok
    }

    public func snapshot() async -> Snapshot {
        guard let docker = dockerPath, await isAvailable() else { return .unavailable }
        async let containersOut = Shell.run(docker, ["ps", "-a", "--format", "{{json .}}", "--size"])
        async let dfOut = Shell.run(docker, ["system", "df", "-v", "--format", "{{json .}}"])
        async let volumesOut = Shell.run(docker, ["volume", "ls", "--format", "{{json .}}"])

        var containers: [Container] = []
        for line in await containersOut.stdout.split(separator: "\n") {
            guard let obj = Self.json(String(line)) else { continue }
            let labels = Self.labels(obj["Labels"] as? String)
            containers.append(Container(
                containerID: obj["ID"] as? String ?? "",
                name: obj["Names"] as? String ?? "",
                image: obj["Image"] as? String ?? "",
                state: obj["State"] as? String ?? "",
                composeProject: labels["com.docker.compose.project"],
                workingDir: labels["com.docker.compose.project.working_dir"],
                sizeBytes: Self.parseSize((obj["Size"] as? String ?? "").split(separator: " ").first.map(String.init) ?? "0")
            ))
        }

        var images: [Image] = []
        var volumes: [Volume] = []
        var buildCache: UInt64 = 0
        if let df = Self.json(await dfOut.stdout) {
            for obj in df["Images"] as? [[String: Any]] ?? [] {
                images.append(Image(
                    imageID: obj["ID"] as? String ?? "",
                    repository: obj["Repository"] as? String ?? "<none>",
                    tag: obj["Tag"] as? String ?? "<none>",
                    sizeBytes: Self.parseSize(obj["Size"] as? String ?? "0"),
                    uniqueBytes: Self.parseSize(obj["UniqueSize"] as? String ?? obj["Size"] as? String ?? "0"),
                    containers: Int(obj["Containers"] as? String ?? "0") ?? 0,
                    created: obj["CreatedSince"] as? String ?? ""
                ))
            }
            let volumeLabels: [String: String?] = Dictionary(uniqueKeysWithValues:
                (await volumesOut.stdout.split(separator: "\n")).compactMap { line -> (String, String?)? in
                    guard let o = Self.json(String(line)), let name = o["Name"] as? String else { return nil }
                    return (name, Self.labels(o["Labels"] as? String)["com.docker.compose.project"])
                })
            for obj in df["Volumes"] as? [[String: Any]] ?? [] {
                let name = obj["Name"] as? String ?? ""
                volumes.append(Volume(
                    name: name,
                    sizeBytes: Self.parseSize(obj["Size"] as? String ?? "0"),
                    links: Int(obj["Links"] as? String ?? "0") ?? 0,
                    composeProject: volumeLabels[name] ?? nil
                ))
            }
            for obj in df["BuildCache"] as? [[String: Any]] ?? [] {
                buildCache += Self.parseSize(obj["Size"] as? String ?? "0")
            }
        }
        return Snapshot(available: true, images: images, volumes: volumes, containers: containers, buildCacheBytes: buildCache)
    }

    public func runningContainersByWorkingDir() async -> [String: Int] {
        let snap = await snapshot()
        var map: [String: Int] = [:]
        for c in snap.containers where c.isRunning {
            if let dir = c.workingDir { map[dir, default: 0] += 1 }
        }
        return map
    }

    public enum DockerError: LocalizedError {
        case unavailable, unknownKind(String), commandFailed(String)
        public var errorDescription: String? {
            switch self {
            case .unavailable: "Docker is not running"
            case .unknownKind(let k): "Unknown docker resource \(k)"
            case .commandFailed(let s): s
            }
        }
    }

    // MARK: - Parsing

    nonisolated static func json(_ text: String) -> [String: Any]? {
        guard let data = text.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    nonisolated static func labels(_ raw: String?) -> [String: String] {
        guard let raw, !raw.isEmpty else { return [:] }
        var out: [String: String] = [:]
        for pair in raw.split(separator: ",") {
            if let eq = pair.firstIndex(of: "=") {
                out[String(pair[..<eq])] = String(pair[pair.index(after: eq)...])
            }
        }
        return out
    }

    /// "1.36GB", "93.6MB", "22.6kB", "0B", "5.22GB (33%)"
    nonisolated static func parseSize(_ text: String) -> UInt64 {
        let s = text.split(separator: " ").first.map(String.init) ?? text
        let scanner = Scanner(string: s)
        guard let value = scanner.scanDouble() else { return 0 }
        let unit = s[scanner.currentIndex...].trimmingCharacters(in: .whitespaces).lowercased()
        let mult: Double
        switch unit {
        case "b": mult = 1
        case "kb": mult = 1_000
        case "mb": mult = 1_000_000
        case "gb": mult = 1_000_000_000
        case "tb": mult = 1_000_000_000_000
        case "kib": mult = 1_024
        case "mib": mult = 1_048_576
        case "gib": mult = 1_073_741_824
        default: mult = 1
        }
        return UInt64(value * mult)
    }
}
