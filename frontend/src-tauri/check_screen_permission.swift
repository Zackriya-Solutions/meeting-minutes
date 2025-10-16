import Cocoa
import ScreenCaptureKit

@main
struct CheckPermission {
    static func main() {
        Task {
            do {
                let content = try await SCShareableContent.current
                print("Screen Recording Permission: GRANTED")
                print("Available displays: \(content.displays.count)")
                exit(0)
            } catch {
                print("Screen Recording Permission: DENIED")
                print("Error: \(error)")
                exit(1)
            }
        }
        RunLoop.main.run()
    }
}
