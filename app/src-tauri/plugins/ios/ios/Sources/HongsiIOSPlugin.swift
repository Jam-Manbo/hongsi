import Foundation
import Tauri
import UIKit
import WebKit
import WidgetKit
import QuickLook
import UniformTypeIdentifiers

private struct ValueArgs: Decodable { let value: String }

final class HongsiIOSPlugin: Plugin, QLPreviewControllerDataSource, QLPreviewControllerDelegate, UIDocumentPickerDelegate {
    private var previewURL: URL?
    private var scopedPreviewURL: URL?
    private var location: HongsiLocation?
    private weak var studentCardWebView: WKWebView?

    @objc override func load(webview: WKWebView) {
        studentCardWebView = webview
        NotificationCenter.default.addObserver(self, selector: #selector(studentCardPause), name: UIApplication.willResignActiveNotification, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(studentCardResume), name: UIApplication.didBecomeActiveNotification, object: nil)
    }
    deinit { NotificationCenter.default.removeObserver(self) }

    @objc private func studentCardPause() {
        studentCardWebView?.evaluateJavaScript("window.dispatchEvent(new Event('hongsi-pause'))", completionHandler: nil)
    }
    @objc private func studentCardResume() {
        studentCardWebView?.evaluateJavaScript("window.dispatchEvent(new Event('hongsi-resume'))", completionHandler: nil)
    }
    @objc func studentCardContext(_ invoke: Invoke) {
        DispatchQueue.main.async { [self] in
            guard let webview = studentCardWebView else { invoke.reject("학생증 QR을 준비하지 못했어요."); return }
            webview.evaluateJavaScript("navigator.userAgent") { agent, error in
                guard error == nil, let agent = agent as? String else { invoke.reject("학생증 QR을 준비하지 못했어요."); return }
                let preferences = UserDefaults.standard
                let key = "hongsi.student-card.device-id"
                let deviceId = preferences.string(forKey: key) ?? UUID().uuidString
                preferences.set(deviceId, forKey: key)
                invoke.resolve([
                    "platform": "ios", "deviceId": deviceId,
                    "userAgent": agent + " Heyoung/1.6.2",
                    "localeVersion": preferences.string(forKey: "hongsi.student-card.locale-version") ?? "",
                    "active": UIApplication.shared.applicationState == .active
                ])
            }
        }
    }
    @objc func saveStudentCardLocale(_ invoke: Invoke) {
        operation(invoke) {
            let value = try invoke.parseArgs(ValueArgs.self).value
            if !value.isEmpty && value.count <= 128 { UserDefaults.standard.set(value, forKey: "hongsi.student-card.locale-version") }
        }
    }

    private func operation(_ invoke: Invoke, _ block: () throws -> Void) {
        do { try block(); invoke.resolve() }
        catch { invoke.reject(error.localizedDescription) }
    }
    @objc func syncWidget(_ invoke: Invoke) {
        operation(invoke) { try WidgetStore.replace(invoke.parseArgs(ValueArgs.self).value); WidgetCenter.shared.reloadAllTimelines() }
    }
    @objc func setWidgetTheme(_ invoke: Invoke) {
        operation(invoke) { try WidgetStore.setTheme(invoke.parseArgs(ValueArgs.self).value); WidgetCenter.shared.reloadAllTimelines() }
    }
    @objc func takeWidgetIntent(_ invoke: Invoke) {
        do { invoke.resolve(try WidgetStore.takeIntent()) } catch { invoke.reject(error.localizedDescription) }
    }
    @objc func acceptWidgetUrl(_ invoke: Invoke) {
        operation(invoke) {
            guard let url = URL(string: try invoke.parseArgs(ValueArgs.self).value) else { return }
            try WidgetStore.accept(url)
        }
    }
    @objc func loadCredentials(_ invoke: Invoke) {
        do { invoke.resolve(["secret": try Credentials.read() as Any? ?? NSNull()]) }
        catch { invoke.reject(error.localizedDescription) }
    }
    @objc func saveCredentials(_ invoke: Invoke) {
        operation(invoke) { try Credentials.write(invoke.parseArgs(ValueArgs.self).value) }
    }
    @objc func clearCredentials(_ invoke: Invoke) {
        operation(invoke) {
            try Credentials.write(nil); try WidgetStore.replace(""); WidgetCenter.shared.reloadAllTimelines()
        }
    }
    @objc func currentLocation(_ invoke: Invoke) {
        Task { @MainActor in
            if location == nil { location = HongsiLocation() }
            do {
                let result = try await location!.locate(requestPermission: true)
                invoke.resolve(["latitude": result.coordinate.latitude, "longitude": result.coordinate.longitude, "accuracy": result.horizontalAccuracy])
            } catch { invoke.reject(error.localizedDescription) }
        }
    }
    private func checkedFile(_ path: String) throws -> URL {
        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("홍시", isDirectory: true).resolvingSymlinksInPath()
        let file = URL(fileURLWithPath: path).resolvingSymlinksInPath()
        guard file.path.hasPrefix(directory.path + "/"), FileManager.default.fileExists(atPath: file.path),
              (try file.resourceValues(forKeys: [.isRegularFileKey])).isRegularFile == true else { throw WidgetFailure("파일이 없어요.") }
        return file
    }
    private func presenter() throws -> UIViewController {
        guard var controller = manager.viewController else { throw WidgetFailure("파일이나 링크를 열지 못했어요.") }
        while let presented = controller.presentedViewController { controller = presented }
        return controller
    }
    @objc func openFile(_ invoke: Invoke) {
        DispatchQueue.main.async { [self] in
            operation(invoke) {
                previewURL = try checkedFile(invoke.parseArgs(ValueArgs.self).value)
                let preview = QLPreviewController(); preview.dataSource = self; preview.delegate = self
                try presenter().present(preview, animated: true)
            }
        }
    }
    @objc func revealFile(_ invoke: Invoke) {
        DispatchQueue.main.async { [self] in
            operation(invoke) {
                let file = try checkedFile(invoke.parseArgs(ValueArgs.self).value)
                let controller = try presenter()
                let browser = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: false)
                browser.directoryURL = file.deletingLastPathComponent()
                browser.shouldShowFileExtensions = true
                browser.delegate = self
                controller.present(browser, animated: true)
            }
        }
    }
    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard let url = urls.first else { return }
        scopedPreviewURL?.stopAccessingSecurityScopedResource()
        scopedPreviewURL = url.startAccessingSecurityScopedResource() ? url : nil
        previewURL = url
        controller.dismiss(animated: true) { [self] in
            guard let presenter = try? presenter() else {
                scopedPreviewURL?.stopAccessingSecurityScopedResource(); scopedPreviewURL = nil; return
            }
            let preview = QLPreviewController(); preview.dataSource = self; preview.delegate = self
            presenter.present(preview, animated: true)
        }
    }
    func previewControllerDidDismiss(_ controller: QLPreviewController) {
        scopedPreviewURL?.stopAccessingSecurityScopedResource(); scopedPreviewURL = nil; previewURL = nil
    }
    @objc func openUrl(_ invoke: Invoke) {
        do {
            let value = try invoke.parseArgs(ValueArgs.self).value
            guard value.count < 16384, let url = URL(string: value), ["https", "http"].contains(url.scheme), url.host != nil else {
                throw WidgetFailure("열 수 없는 주소예요.")
            }
            DispatchQueue.main.async {
                UIApplication.shared.open(url, options: [:]) { opened in
                    if opened { invoke.resolve() } else { invoke.reject("파일이나 링크를 열지 못했어요.") }
                }
            }
        } catch { invoke.reject(error.localizedDescription) }
    }
    func numberOfPreviewItems(in controller: QLPreviewController) -> Int { previewURL == nil ? 0 : 1 }
    func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem { previewURL! as NSURL }
}

@_cdecl("init_plugin_hongsi_ios")
func initPlugin() -> Plugin { HongsiIOSPlugin() }
