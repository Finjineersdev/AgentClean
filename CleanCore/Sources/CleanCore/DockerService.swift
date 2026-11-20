import Foundation

/// Reads containers and volume sizes from the `docker` CLI, read-only, so
/// Dev Stack can show database containers and their compose volumes.
public actor DockerService {
    public static let shared = DockerService()
    public nonisolated let scheme = "docker"

    public struct Volume: Sendable, Identifiable {
        public var id: String { name }
        public let name: String
        public let sizeBytes: UInt64
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
        public var isRunning: Bool { state.lowercased() == "running" }
    }

    public struct Snapshot: Sendable {
        public let available: Bool
        public let volumes: [Volume]
        public let containers: [Container]
        public static let unavailable = Snapshot(available: false, volumes: [], containers: [])
    }

    private var dockerPath: String? { Shell.which("docker") }

    private func isAvailable() async -> Bool {
        guard let docker = dockerPath else { return false }
        return await Shell.run(docker, ["info", "--format", "{{.ServerVersion}}"], timeout: 10).ok
    }

    public func snapshot() async -> Snapshot {
        guard let docker = dockerPath, await isAvailable() else { return .unavailable }
        async let containersOut = Shell.run(docker, ["ps", "-a", "--format", "{{json .}}"])
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
                workingDir: labels["com.docker.compose.project.working_dir"]
            ))
        }

        var volumes: [Volume] = []
        if let df = Self.json(await dfOut.stdout) {
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
                    composeProject: volumeLabels[name] ?? nil
                ))
            }
        }
        return Snapshot(available: true, volumes: volumes, containers: containers)
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
