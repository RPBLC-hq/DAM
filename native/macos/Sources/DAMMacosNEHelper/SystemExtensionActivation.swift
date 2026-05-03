import Foundation
import SystemExtensions

final class SystemExtensionActivation: NSObject, OSSystemExtensionRequestDelegate, @unchecked Sendable {
    private let semaphore = DispatchSemaphore(value: 0)
    private var result: Result<String, Error>?
    private var requiredUserApproval = false

    func activate(bundleIdentifier: String) throws -> String {
        let request = OSSystemExtensionRequest.activationRequest(
            forExtensionWithIdentifier: bundleIdentifier,
            queue: .main
        )
        request.delegate = self
        DispatchQueue.main.async {
            OSSystemExtensionManager.shared.submitRequest(request)
        }

        while semaphore.wait(timeout: .now() + 0.1) == .timedOut {
            RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.1))
        }

        switch result {
        case .success(let message):
            return message
        case .failure(let error):
            throw error
        case .none:
            throw ActivationError.missingResult
        }
    }

    func request(
        _ request: OSSystemExtensionRequest,
        actionForReplacingExtension existing: OSSystemExtensionProperties,
        withExtension replacement: OSSystemExtensionProperties
    ) -> OSSystemExtensionRequest.ReplacementAction {
        .replace
    }

    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) {
        requiredUserApproval = true
    }

    func request(
        _ request: OSSystemExtensionRequest,
        didFinishWithResult result: OSSystemExtensionRequest.Result
    ) {
        let approval = requiredUserApproval ? " after user approval" : ""
        self.result = .success("system extension activation finished\(approval): \(result)")
        semaphore.signal()
    }

    func request(_ request: OSSystemExtensionRequest, didFailWithError error: Error) {
        result = .failure(error)
        semaphore.signal()
    }

    enum ActivationError: Error, CustomStringConvertible {
        case missingResult

        var description: String {
            "system extension activation finished without a result"
        }
    }
}
