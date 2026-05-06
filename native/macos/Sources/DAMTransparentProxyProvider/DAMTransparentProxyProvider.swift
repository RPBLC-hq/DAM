import DAMNetworkExtensionSupport
import Foundation
import Network
import NetworkExtension

public final class DAMTransparentProxyProvider: NETransparentProxyProvider, @unchecked Sendable {
    private let stateQueue = DispatchQueue(label: "com.rpblc.dam.network-extension.provider.state")
    private var activeFlows: [UUID: TCPFlowProxy] = [:]
    private var runtimeConfiguration = DAMProxyRuntimeConfiguration()

    public override func startProxy(
        options: [String: Any]? = nil,
        completionHandler: @escaping (Error?) -> Void
    ) {
        let completion = SendableCompletion(completionHandler)
        let providerConfiguration = (protocolConfiguration as? NETunnelProviderProtocol)?.providerConfiguration
        runtimeConfiguration = DAMProxyRuntimeConfiguration(providerConfiguration: providerConfiguration)

        let settings = NETransparentProxyNetworkSettings(tunnelRemoteAddress: "127.0.0.1")
        settings.includedNetworkRules = [
            NENetworkRule(
                remoteNetworkEndpoint: nil,
                remotePrefix: 0,
                localNetworkEndpoint: nil,
                localPrefix: 0,
                protocol: .any,
                direction: .outbound
            ),
        ]

        setTunnelNetworkSettings(settings) { error in
            completion.call(error)
        }
    }

    public override func stopProxy(
        with reason: NEProviderStopReason,
        completionHandler: @escaping () -> Void
    ) {
        stateQueue.sync {
            for flow in activeFlows.values {
                flow.cancel()
            }
            activeFlows.removeAll()
        }
        completionHandler()
    }

    public override func handleNewFlow(_ flow: NEAppProxyFlow) -> Bool {
        if runtimeConfiguration.shouldBypassSource(signingIdentifier: flow.metaData.sourceAppSigningIdentifier) {
            return false
        }
        guard let tcpFlow = flow as? NEAppProxyTCPFlow,
              let endpoint = FlowEndpoint(tcpFlow: tcpFlow),
              runtimeConfiguration.shouldProtect(host: endpoint.host)
        else {
            return false
        }

        let proxy = TCPFlowProxy(
            flow: tcpFlow,
            endpoint: endpoint,
            runtimeConfiguration: runtimeConfiguration
        ) { [weak self] id in
            guard let provider = self else {
                return
            }
            provider.stateQueue.async {
                provider.activeFlows.removeValue(forKey: id)
            }
        }

        stateQueue.async {
            self.activeFlows[proxy.id] = proxy
            proxy.start()
        }
        return true
    }

}

private struct SendableCompletion: @unchecked Sendable {
    private let handler: (Error?) -> Void

    init(_ handler: @escaping (Error?) -> Void) {
        self.handler = handler
    }

    func call(_ error: Error?) {
        handler(error)
    }
}
