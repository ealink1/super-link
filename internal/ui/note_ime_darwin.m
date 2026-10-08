//go:build darwin && cgo

#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <stdint.h>

static const char SuperLinkNoteIMEKey;
static IMP SuperLinkOriginalIMERect;

// GLFW's default implementation returns its view origin instead of a screen
// caret rectangle. Override that query only while our rich editor has focus.
static NSRect SuperLinkNoteIMERect(id view, SEL selector, NSRange range, NSRangePointer actualRange) {
    NSValue *value = objc_getAssociatedObject(view, &SuperLinkNoteIMEKey);
    if (value && [view window]) {
        if (actualRange) *actualRange = range;
        NSRect windowRect = [view convertRect:[value rectValue] toView:nil];
        return [[view window] convertRectToScreen:windowRect];
    }
    return ((NSRect (*)(id, SEL, NSRange, NSRangePointer))SuperLinkOriginalIMERect)(view, selector, range, actualRange);
}

void superlink_note_ime(uintptr_t pointer, double x, double y, double height, double width, double canvasHeight, int active) {
    if (!pointer || width <= 0 || canvasHeight <= 0) return;
    NSWindow *window = (NSWindow *)pointer;
    dispatch_async(dispatch_get_main_queue(), ^{
        NSView *view = window.contentView;
        // Avoid changing the text-input implementation of unrelated native views.
        if (![NSStringFromClass([view class]) isEqualToString:@"GLFWContentView"]) return;
        static dispatch_once_t once;
        dispatch_once(&once, ^{
            SEL selector = @selector(firstRectForCharacterRange:actualRange:);
            Method method = class_getInstanceMethod([view class], selector);
            if (method) SuperLinkOriginalIMERect = method_setImplementation(method, (IMP)SuperLinkNoteIMERect);
        });
        if (!SuperLinkOriginalIMERect) return;
        if (!active) {
            objc_setAssociatedObject(view, &SuperLinkNoteIMEKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            return;
        }
        NSRect bounds = view.bounds;
        CGFloat sx = NSWidth(bounds) / width, sy = NSHeight(bounds) / canvasHeight;
        CGFloat caretHeight = MAX(1, height * sy);
        CGFloat caretY = view.isFlipped ? y * sy : NSHeight(bounds) - y * sy - caretHeight;
        NSRect rect = NSMakeRect(x * sx, caretY, MAX(1, sx), caretHeight);
        NSValue *previous = objc_getAssociatedObject(view, &SuperLinkNoteIMEKey);
        if (previous && NSEqualRects([previous rectValue], rect)) return;
        objc_setAssociatedObject(view, &SuperLinkNoteIMEKey, [NSValue valueWithRect:rect], OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        [[view inputContext] invalidateCharacterCoordinates];
    });
}
