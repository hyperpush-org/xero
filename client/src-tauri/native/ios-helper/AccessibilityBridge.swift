// AccessibilityBridge.swift — AXUIElement inspection for native Swift apps.
//
// Uses the macOS Accessibility API (AXUIElement) to inspect the Simulator
// process's UI element tree. This provides element-at-point inspection for
// native iOS apps (SwiftUI, UIKit) that don't have a Metro inspector.
//
// The AXUIElement API sees the Simulator.app's AX tree from the host Mac
// perspective. Elements contain: role, title, value, identifier, enabled,
// focused, and frame (in screen coordinates).
//
// Requires Accessibility permission (same as cg_input.rs).

import Foundation
import ApplicationServices
import AppKit

class AccessibilityBridge {

    /// Dump the full accessibility tree of the Simulator process.
    /// Returns a JSON-serializable dictionary matching the UiNode shape
    /// expected by the Rust `normalize_tree()` function in ios_ui.rs.
    func dumpTree() -> Result<[String: Any], HelperError> {
        guard let simPid = findSimulatorPid() else {
            return .failure(.captureError("Simulator.app not running"))
        }

        let appElement = AXUIElementCreateApplication(simPid)

        // Get the main window's content area.
        var windowValue: CFTypeRef?
        let windowResult = AXUIElementCopyAttributeValue(appElement, kAXFocusedWindowAttribute as CFString, &windowValue)

        let rootElement: AXUIElement
        if windowResult == .success, let window = windowValue {
            rootElement = (window as! AXUIElement)
        } else {
            // Fallback: use the app element directly.
            rootElement = appElement
        }

        guard let tree = elementToDict(rootElement, depth: 0) else {
            return .failure(.captureError("failed to read AX tree from Simulator"))
        }

        return .success(tree)
    }

    /// Get the element at a specific screen coordinate.
    /// Returns a dictionary matching the UiNode shape.
    func elementAtPoint(x: Float, y: Float) -> Result<[String: Any], HelperError> {
        guard let simPid = findSimulatorPid() else {
            return .failure(.captureError("Simulator.app not running"))
        }

        let appElement = AXUIElementCreateApplication(simPid)
        var elementRef: AXUIElement?
        let point = CGPoint(x: CGFloat(x), y: CGFloat(y))

        let result = AXUIElementCopyElementAtPosition(appElement, Float(point.x), Float(point.y), &elementRef)

        guard result == .success, let element = elementRef else {
            return .failure(.captureError("no AX element at (\(x), \(y))"))
        }

        guard let dict = elementToDict(element, depth: 0) else {
            return .failure(.captureError("failed to read AX element properties"))
        }

        return .success(dict)
    }

    // MARK: - Private

    private func findSimulatorPid() -> pid_t? {
        let workspace = NSWorkspace.shared
        for app in workspace.runningApplications {
            if app.bundleIdentifier == "com.apple.iphonesimulator" {
                return app.processIdentifier
            }
        }
        return nil
    }

    /// Convert an AXUIElement to a dictionary matching the format that
    /// `normalize_tree()` in ios_ui.rs expects:
    /// {
    ///   "type": "AXButton",          // maps to platform_role
    ///   "identifier": "...",          // AX identifier
    ///   "label": "...",               // AX title/description
    ///   "value": "...",               // AX value
    ///   "enabled": true,
    ///   "focused": false,
    ///   "frame": { "x": .., "y": .., "w": .., "h": .. },
    ///   "children": [ ... ]
    /// }
    private func elementToDict(_ element: AXUIElement, depth: Int) -> [String: Any]? {
        guard depth < 40 else { return nil } // Prevent infinite recursion.

        var dict: [String: Any] = [:]

        // Role
        if let role = axString(element, kAXRoleAttribute as CFString) {
            dict["type"] = role
        } else {
            dict["type"] = "AXUnknown"
        }

        // Identifier (AXIdentifier)
        if let identifier = axString(element, kAXIdentifierAttribute as CFString) {
            dict["identifier"] = identifier
        } else {
            dict["identifier"] = ""
        }

        // Label: prefer AXTitle, fall back to AXDescription
        if let title = axString(element, kAXTitleAttribute as CFString), !title.isEmpty {
            dict["label"] = title
        } else if let desc = axString(element, kAXDescriptionAttribute as CFString), !desc.isEmpty {
            dict["label"] = desc
        } else {
            dict["label"] = ""
        }

        // Value
        if let value = axString(element, kAXValueAttribute as CFString) {
            dict["value"] = value
        } else {
            dict["value"] = ""
        }

        // Enabled
        dict["enabled"] = axBool(element, kAXEnabledAttribute as CFString) ?? true

        // Focused
        dict["focused"] = axBool(element, kAXFocusedAttribute as CFString) ?? false

        // Frame (screen coordinates)
        let frame = axFrame(element)
        dict["frame"] = [
            "x": frame.origin.x,
            "y": frame.origin.y,
            "w": frame.size.width,
            "h": frame.size.height,
        ]

        // Children (recursive)
        var children: [[String: Any]] = []
        if let axChildren = axArray(element, kAXChildrenAttribute as CFString) {
            for child in axChildren {
                let childElement = child as! AXUIElement
                if let childDict = elementToDict(childElement, depth: depth + 1) {
                    children.append(childDict)
                }
            }
        }
        dict["children"] = children

        return dict
    }

    // MARK: - AX attribute helpers

    private func axString(_ element: AXUIElement, _ attribute: CFString) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success else {
            return nil
        }
        return value as? String
    }

    private func axBool(_ element: AXUIElement, _ attribute: CFString) -> Bool? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success else {
            return nil
        }
        if let num = value as? NSNumber {
            return num.boolValue
        }
        return nil
    }

    private func axArray(_ element: AXUIElement, _ attribute: CFString) -> [AnyObject]? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success else {
            return nil
        }
        return value as? [AnyObject]
    }

    private func axFrame(_ element: AXUIElement) -> CGRect {
        var position = CGPoint.zero
        var size = CGSize.zero

        var posValue: CFTypeRef?
        if AXUIElementCopyAttributeValue(element, kAXPositionAttribute as CFString, &posValue) == .success,
           let posValue = posValue {
            var point = CGPoint.zero
            AXValueGetValue(posValue as! AXValue, .cgPoint, &point)
            position = point
        }

        var sizeValue: CFTypeRef?
        if AXUIElementCopyAttributeValue(element, kAXSizeAttribute as CFString, &sizeValue) == .success,
           let sizeValue = sizeValue {
            var s = CGSize.zero
            AXValueGetValue(sizeValue as! AXValue, .cgSize, &s)
            size = s
        }

        return CGRect(origin: position, size: size)
    }
}
