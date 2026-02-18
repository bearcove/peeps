import Foundation
import RoamRuntime

private struct NoopDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        requestId _: UInt64,
        registry _: ChannelRegistry,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        // Intentionally send no response so Rust side keeps waiting.
    }
}

private enum PeerError: Error {
    case missingPeerAddr
    case invalidPeerAddr(String)
}

private func parsePeerAddr(_ value: String) throws -> (host: String, port: Int) {
    guard let colonIdx = value.lastIndex(of: ":") else {
        throw PeerError.invalidPeerAddr(value)
    }

    let host = String(value[..<colonIdx])
    let portText = String(value[value.index(after: colonIdx)...])
    guard !host.isEmpty, let port = Int(portText) else {
        throw PeerError.invalidPeerAddr(value)
    }

    return (host, port)
}

@main
struct RustSwiftPeer {
    static func main() async {
        do {
            try await run()
        } catch {
            fputs("rust_swift_peer failed: \(error)\n", stderr)
            exit(1)
        }
    }

    private static func run() async throws {
        guard let peerAddr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
            throw PeerError.missingPeerAddr
        }

        let (host, port) = try parsePeerAddr(peerAddr)
        let transport = try await connect(host: host, port: port)
        let (_, driver) = try await establishInitiator(
            transport: transport,
            dispatcher: NoopDispatcher()
        )
        try await driver.run()
    }
}
