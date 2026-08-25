#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>

#import "screen_capture_kit.h"

static void write_error(char *buffer, size_t size, NSString *message) {
    if (buffer == NULL || size == 0) return;
    const char *text = message.UTF8String ?: "未知的 ScreenCaptureKit 錯誤";
    snprintf(buffer, size, "%s", text);
}

API_AVAILABLE(macos(15.0))
@interface SCKRecorder : NSObject <SCStreamDelegate, SCRecordingOutputDelegate>
@property(nonatomic, strong) SCStream *stream;
@property(nonatomic, strong) SCRecordingOutput *recordingOutput;
@property(nonatomic, strong) dispatch_semaphore_t startSemaphore;
@property(nonatomic, strong) dispatch_semaphore_t finishSemaphore;
@property(nonatomic, strong) NSError *startupError;
@property(nonatomic, strong) NSError *runtimeError;
@property(nonatomic, assign) BOOL didStart;
@property(nonatomic, assign) BOOL didFinish;
@end

@implementation SCKRecorder
- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    self.runtimeError = error;
    if (!self.didFinish) {
        self.didFinish = YES;
        dispatch_semaphore_signal(self.finishSemaphore);
    }
}
- (void)recordingOutputDidStartRecording:(SCRecordingOutput *)recordingOutput {
    self.didStart = YES;
    dispatch_semaphore_signal(self.startSemaphore);
}
- (void)recordingOutput:(SCRecordingOutput *)recordingOutput didFailWithError:(NSError *)error {
    self.startupError = error;
    if (!self.didStart) dispatch_semaphore_signal(self.startSemaphore);
    if (!self.didFinish) {
        self.didFinish = YES;
        dispatch_semaphore_signal(self.finishSemaphore);
    }
}
- (void)recordingOutputDidFinishRecording:(SCRecordingOutput *)recordingOutput {
    if (!self.didFinish) {
        self.didFinish = YES;
        dispatch_semaphore_signal(self.finishSemaphore);
    }
}
@end

static SCKRecorder *active_recorder = nil;
static dispatch_queue_t recorder_queue;

static void initialize_recorder_queue(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{ recorder_queue = dispatch_queue_create("com.taichi.screenshot.sck", DISPATCH_QUEUE_SERIAL); });
}

bool sck_start_recording(
    unsigned long display_index,
    double monitor_x, double monitor_y, double monitor_width, double monitor_height,
    double x, double y, double width, double height,
    double canvas_width, double canvas_height,
    unsigned int fps,
    bool capture_microphone,
    bool capture_system_audio,
    const char *output_path,
    char *error_buffer,
    size_t error_buffer_size
) {
    if (__builtin_available(macOS 15.0, *)) {
        initialize_recorder_queue();
        __block NSString *failure = nil;
        dispatch_sync(recorder_queue, ^{
            if (active_recorder != nil) { failure = @"已有錄影正在進行中"; return; }
            SCKRecorder *recorder = [SCKRecorder new];
            recorder.startSemaphore = dispatch_semaphore_create(0);
            recorder.finishSemaphore = dispatch_semaphore_create(0);
            active_recorder = recorder;

            // Include hidden windows in the inventory: the capture overlay is
            // hidden immediately before recording starts and is shown again
            // as the stop control. It still must be present in the exclusion
            // list even while hidden.
            [SCShareableContent getShareableContentExcludingDesktopWindows:NO onScreenWindowsOnly:NO completionHandler:^(SCShareableContent *content, NSError *error) {
                if (error != nil || content == nil) {
                    recorder.startupError = error ?: [NSError errorWithDomain:@"sck" code:1 userInfo:@{NSLocalizedDescriptionKey: @"無法取得可錄製的螢幕內容"}];
                    dispatch_semaphore_signal(recorder.startSemaphore);
                    return;
                }
                if (content.displays.count == 0) {
                    recorder.startupError = [NSError errorWithDomain:@"sck" code:2 userInfo:@{NSLocalizedDescriptionKey: @"找不到可錄製的螢幕"}];
                    dispatch_semaphore_signal(recorder.startSemaphore);
                    return;
                }
                // Tauri and ScreenCaptureKit enumerate displays independently;
                // their array indices are not stable across multi-display
                // arrangements.  Width/height alone are also ambiguous when
                // two displays share the same resolution. Match the physical
                // monitor bounds first, using the index only as a last resort.
                CGFloat overlayWidth = MAX((CGFloat)canvas_width, 1.0);
                CGFloat overlayHeight = MAX((CGFloat)canvas_height, 1.0);
                NSUInteger selectedDisplayIndex = MIN(display_index, content.displays.count - 1);
                CGFloat bestScore = CGFLOAT_MAX;
                for (NSUInteger index = 0; index < content.displays.count; index++) {
                    SCDisplay *candidate = content.displays[index];
                    CGRect bounds = CGDisplayBounds(candidate.displayID);
                    CGFloat sizeScore = fabs((CGFloat)candidate.width - (CGFloat)monitor_width)
                        + fabs((CGFloat)candidate.height - (CGFloat)monitor_height);
                    CGFloat positionScore = fabs(bounds.origin.x - (CGFloat)monitor_x)
                        + fabs(bounds.origin.y - (CGFloat)monitor_y);
                    // Position is the identity signal. Keep size as a tie
                    // breaker because Tauri and Quartz may differ by scale.
                    CGFloat score = positionScore * 1000.0 + sizeScore;
                    if (score < bestScore) {
                        bestScore = score;
                        selectedDisplayIndex = index;
                    }
                }
                SCDisplay *display = content.displays[selectedDisplayIndex];
                CGRect selectedBounds = CGDisplayBounds(display.displayID);
                fprintf(stderr, "[sck] display_match requested=%lu target=(%.0f,%.0f %.0fx%.0f) selected=%lu selected_id=%u bounds=(%.0f,%.0f %.0fx%.0f) overlay=%.0fx%.0f score=%.1f candidates=",
                    display_index, monitor_x, monitor_y, monitor_width, monitor_height,
                    (unsigned long)selectedDisplayIndex, display.displayID,
                    selectedBounds.origin.x, selectedBounds.origin.y, selectedBounds.size.width, selectedBounds.size.height,
                    overlayWidth, overlayHeight, bestScore);
                for (NSUInteger index = 0; index < content.displays.count; index++) {
                    SCDisplay *candidate = content.displays[index];
                    fprintf(stderr, "%s%lux%lu", index == 0 ? "" : ",", (unsigned long)candidate.width, (unsigned long)candidate.height);
                }
                fprintf(stderr, "\n");
                SCRunningApplication *ownApp = nil;
                NSMutableArray<SCWindow *> *ownWindows = [NSMutableArray array];
                pid_t pid = NSProcessInfo.processInfo.processIdentifier;
                for (SCRunningApplication *application in content.applications) {
                    if (application.processID == pid) { ownApp = application; break; }
                }
                for (SCWindow *window in content.windows) {
                    if (window.owningApplication.processID == pid) [ownWindows addObject:window];
                }
                // Exclude concrete windows, not only the application object.
                // Tauri's dynamically-created WebView windows are not always
                // present in SCShareableContent.applications, but are present
                // in SCShareableContent.windows.
                SCContentFilter *filter = ownWindows.count > 0
                    ? [[SCContentFilter alloc] initWithDisplay:display excludingWindows:ownWindows]
                    : (ownApp != nil
                        ? [[SCContentFilter alloc] initWithDisplay:display excludingApplications:@[ownApp] exceptingWindows:@[]]
                        : [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]]);
                SCStreamConfiguration *configuration = [SCStreamConfiguration new];
                // sourceRect is in the selected display's local logical
                // coordinate system; output dimensions are pixels.
                CGFloat displayWidth = MAX((CGFloat)display.width, 1.0);
                CGFloat displayHeight = MAX((CGFloat)display.height, 1.0);
                // The selection canvas is sized by the Tauri WebView, whereas
                // ScreenCaptureKit expects points in SCDisplay's own logical
                // coordinate space.  They are not necessarily equal on macOS
                // (menu bar, Dock and Retina arrangements make the heights
                // differ), so convert proportionally before applying bounds.
                CGFloat sourceScaleX = displayWidth / overlayWidth;
                CGFloat sourceScaleY = displayHeight / overlayHeight;
                CGFloat requestedX = x * sourceScaleX;
                CGFloat requestedY = y * sourceScaleY;
                CGFloat requestedWidth = width * sourceScaleX;
                CGFloat requestedHeight = height * sourceScaleY;
                CGFloat sourceX = MAX(0.0, MIN(requestedX, displayWidth));
                CGFloat sourceY = MAX(0.0, MIN(requestedY, displayHeight));
                CGFloat sourceWidth = MAX(1.0, MIN(requestedWidth, displayWidth - sourceX));
                CGFloat sourceHeight = MAX(1.0, MIN(requestedHeight, displayHeight - sourceY));
                CGFloat outputScale = (CGFloat)CGDisplayPixelsWide(display.displayID) / displayWidth;
                configuration.sourceRect = CGRectMake(sourceX, sourceY, sourceWidth, sourceHeight);
                configuration.scalesToFit = YES;
                configuration.width = MAX(2, (NSInteger)llround(sourceWidth * outputScale));
                configuration.height = MAX(2, (NSInteger)llround(sourceHeight * outputScale));
                fprintf(stderr, "[sck] display=%lu bounds=%.0fx%.0f overlay=%.0fx%.0f source=(%.1f,%.1f %.1fx%.1f) source_scale=%.3fx%.3f output=%ldx%ld own_windows=%lu own_app=%s\n",
                    (unsigned long)selectedDisplayIndex, displayWidth, displayHeight, overlayWidth, overlayHeight, sourceX, sourceY, sourceWidth, sourceHeight,
                    sourceScaleX, sourceScaleY, (long)configuration.width, (long)configuration.height,
                    (unsigned long)ownWindows.count, ownApp != nil ? "yes" : "no");
                configuration.minimumFrameInterval = CMTimeMake(1, MAX(1, (int32_t)fps));
                configuration.queueDepth = 5;
                configuration.showsCursor = YES;
                configuration.capturesAudio = capture_system_audio;
                configuration.captureMicrophone = capture_microphone;

                SCRecordingOutputConfiguration *outputConfiguration = [SCRecordingOutputConfiguration new];
                outputConfiguration.outputURL = [NSURL fileURLWithPath:[NSString stringWithUTF8String:output_path]];
                outputConfiguration.videoCodecType = AVVideoCodecTypeH264;
                outputConfiguration.outputFileType = AVFileTypeMPEG4;
                recorder.recordingOutput = [[SCRecordingOutput alloc] initWithConfiguration:outputConfiguration delegate:recorder];
                recorder.stream = [[SCStream alloc] initWithFilter:filter configuration:configuration delegate:recorder];
                NSError *addError = nil;
                if (![recorder.stream addRecordingOutput:recorder.recordingOutput error:&addError]) {
                    recorder.startupError = addError;
                    dispatch_semaphore_signal(recorder.startSemaphore);
                    return;
                }
                [recorder.stream startCaptureWithCompletionHandler:^(NSError *startError) {
                    if (startError != nil) {
                        recorder.startupError = startError;
                        dispatch_semaphore_signal(recorder.startSemaphore);
                    }
                }];
            }];

            long result = dispatch_semaphore_wait(recorder.startSemaphore, dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC));
            if (result != 0) failure = @"啟動 ScreenCaptureKit 錄影逾時";
            else if (recorder.startupError != nil) failure = recorder.startupError.localizedDescription;
            else if (!recorder.didStart) failure = @"ScreenCaptureKit 未回報錄影開始";
            if (failure != nil) active_recorder = nil;
        });
        if (failure != nil) { write_error(error_buffer, error_buffer_size, failure); return false; }
        return true;
    }
    write_error(error_buffer, error_buffer_size, @"ScreenCaptureKit 檔案錄製需要 macOS 15 或以上版本");
    return false;
}

bool sck_stop_recording(char *error_buffer, size_t error_buffer_size) {
    if (!__builtin_available(macOS 15.0, *)) { write_error(error_buffer, error_buffer_size, @"目前 macOS 版本不支援"); return false; }
    initialize_recorder_queue();
    __block NSString *failure = nil;
    dispatch_sync(recorder_queue, ^{
        SCKRecorder *recorder = active_recorder;
        if (recorder == nil) { failure = @"目前沒有進行中的錄影"; return; }
        [recorder.stream stopCaptureWithCompletionHandler:^(NSError *error) {
            if (error != nil) recorder.runtimeError = error;
        }];
        long result = dispatch_semaphore_wait(recorder.finishSemaphore, dispatch_time(DISPATCH_TIME_NOW, 15 * NSEC_PER_SEC));
        if (result != 0) failure = @"等待 ScreenCaptureKit 完成 MP4 封裝逾時";
        else if (recorder.runtimeError != nil) failure = recorder.runtimeError.localizedDescription;
        active_recorder = nil;
    });
    if (failure != nil) { write_error(error_buffer, error_buffer_size, failure); return false; }
    return true;
}
