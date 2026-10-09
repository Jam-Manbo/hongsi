import Foundation
import CoreLocation

@MainActor
final class HongsiLocation: NSObject, @preconcurrency CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private var continuation: CheckedContinuation<CLLocation, Error>?
    private var timeout: Task<Void, Never>?
    private var mayRequestPermission = false

    override init() { super.init(); manager.delegate = self; manager.desiredAccuracy = kCLLocationAccuracyBest }
    func locate(requestPermission: Bool) async throws -> CLLocation {
        guard continuation == nil else { throw WidgetFailure("위치를 확인하고 있어요. 잠시 후 다시 시도해 주세요.") }
        mayRequestPermission = requestPermission
        return try await withCheckedThrowingContinuation { pending in
            continuation = pending
            timeout = Task { [weak self] in
                try? await Task.sleep(nanoseconds: 15_000_000_000)
                if !Task.isCancelled { self?.finish(.failure(WidgetFailure("위치를 찾지 못했어요."))) }
            }
            request()
        }
    }
    private func request() {
        guard continuation != nil else { return }
        switch manager.authorizationStatus {
        case .authorizedAlways, .authorizedWhenInUse:
            if let location = manager.location, location.horizontalAccuracy >= 0, location.horizontalAccuracy <= 100,
               abs(location.timestamp.timeIntervalSinceNow) <= 30 { finish(.success(location)) }
            else { manager.requestLocation() }
        case .notDetermined:
            if mayRequestPermission { manager.requestWhenInUseAuthorization() }
            else { finish(.failure(WidgetFailure("앱에서 출석 화면을 열어 위치 권한을 허용해 주세요."))) }
        default: finish(.failure(WidgetFailure("설정에서 홍시의 위치 권한을 허용해 주세요.")))
        }
    }
    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) { request() }
    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last, location.horizontalAccuracy >= 0, abs(location.timestamp.timeIntervalSinceNow) <= 30 else {
            finish(.failure(WidgetFailure("위치를 찾지 못했어요."))); return
        }
        finish(.success(location))
    }
    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) { finish(.failure(WidgetFailure("위치를 찾지 못했어요."))) }
    private func finish(_ result: Result<CLLocation, Error>) {
        let pending = continuation; continuation = nil; timeout?.cancel(); timeout = nil
        manager.stopUpdatingLocation(); pending?.resume(with: result)
    }
}
