//go:build darwin && cgo

#import <AppKit/AppKit.h>
#import <stdint.h>

extern void superlinkApplicationAction(uintptr_t callback, int quit);

// Preserve GLFW's delegate and forward all unrelated AppKit events to it.
// Replacing only these lifecycle decisions avoids changing Fyne's event loop.
@interface SuperLinkApplicationDelegate : NSObject <NSApplicationDelegate>
@property(nonatomic, retain) id original;
@property(nonatomic, assign) uintptr_t callback;
@end

@implementation SuperLinkApplicationDelegate
- (BOOL)respondsToSelector:(SEL)selector {
    return [super respondsToSelector:selector] || [self.original respondsToSelector:selector];
}
- (id)forwardingTargetForSelector:(SEL)selector {
    if ([self.original respondsToSelector:selector]) return self.original;
    return [super forwardingTargetForSelector:selector];
}
- (BOOL)applicationShouldHandleReopen:(NSApplication *)sender hasVisibleWindows:(BOOL)visible {
    if (self.callback) superlinkApplicationAction(self.callback, 0);
    return NO; // The Fyne UI queue restores and focuses the existing window.
}
- (NSApplicationTerminateReply)applicationShouldTerminate:(NSApplication *)sender {
    if (self.callback) superlinkApplicationAction(self.callback, 1);
    return NSTerminateCancel; // Save and release services before Fyne quits.
}
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return NO;
}
- (void)dealloc {
    [_original release];
    [super dealloc];
}
@end

static SuperLinkApplicationDelegate *lifecycle;

static void SuperLinkLifecycleOnMain(void (^run)(void)) {
    if ([NSThread isMainThread]) run();
    else dispatch_sync(dispatch_get_main_queue(), run);
}

int superlink_lifecycle_install(uintptr_t callback) {
    __block int installed = 0;
    SuperLinkLifecycleOnMain(^{
        if (!NSApp.delegate || lifecycle) return;
        lifecycle = [[SuperLinkApplicationDelegate alloc] init];
        lifecycle.original = NSApp.delegate;
        lifecycle.callback = callback;
        NSApp.delegate = lifecycle;
        installed = 1;
    });
    return installed;
}

void superlink_lifecycle_remove(uintptr_t callback) {
    SuperLinkLifecycleOnMain(^{
        if (!lifecycle || lifecycle.callback != callback) return;
        lifecycle.callback = 0;
        if (NSApp.delegate == lifecycle) NSApp.delegate = lifecycle.original;
        [lifecycle release];
        lifecycle = nil;
    });
}
