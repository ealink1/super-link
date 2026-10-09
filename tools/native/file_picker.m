#import <AppKit/AppKit.h>
#include <stdio.h>
#include <string.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSApplication *app = [NSApplication sharedApplication];
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [app finishLaunching];
        [app activateIgnoringOtherApps:YES];
        NSOpenPanel *panel = [NSOpenPanel openPanel];
        BOOL directory = argc > 1 && strcmp(argv[1], "directory") == 0;
        [panel setTitle:argc > 2 ? [NSString stringWithUTF8String:argv[2]] : @"选择文件"];
        [panel setCanChooseFiles:!directory];
        
        [panel setCanChooseDirectories:directory];
        [panel setAllowsMultipleSelection:NO];
        NSMutableArray<UTType *> *types = [NSMutableArray array];
        for (int i = 3; i < argc; i++) {
            NSString *extension = [NSString stringWithUTF8String:argv[i]];
            UTType *type = [UTType typeWithFilenameExtension:extension];
            if (type) [types addObject:type];
        }
        if ([types count] > 0 && !directory) [panel setAllowedContentTypes:types];
        [panel setAllowsOtherFileTypes:NO];
        if ([panel runModal] == NSModalResponseOK) {
            const char *path = [[[panel URL] path] UTF8String];
            if (path) fwrite(path, 1, strlen(path), stdout);
        }
        return 0;
    }
}
