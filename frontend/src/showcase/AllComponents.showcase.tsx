import * as Production000 from '../components/AISummary/Block';
import * as Production001 from '../components/AISummary/BlockNoteSummaryView';
import * as Production002 from '../components/AISummary/Section';
import * as Production003 from '../components/AISummary/index';
import * as Production004 from '../components/About';
import * as Production005 from '../components/AnalyticsConsentSwitch';
import * as Production006 from '../components/AnalyticsDataModal';
import * as Production007 from '../components/AnalyticsProvider';
import * as Production008 from '../components/AppSidebar';
import * as Production009 from '../components/AudioBackendSelector';
import * as Production010 from '../components/AudioLevelMeter';
import * as Production011 from '../components/AutoMeetingDetection';
import * as Production012 from '../components/BetaSettings';
import * as Production013 from '../components/BlockNoteEditor/BasicBlockNoteTest';
import * as Production014 from '../components/BlockNoteEditor/Editor';
import * as Production015 from '../components/BluetoothPlaybackWarning';
import * as Production016 from '../components/BuiltInModelManager';
import * as Production017 from '../components/CalendarSettings';
import * as Production018 from '../components/ChunkProgressDisplay';
import * as Production019 from '../components/ComplianceNotification';
import * as Production020 from '../components/ConfidenceIndicator';
import * as Production021 from '../components/ConfirmationModel/confirmation-modal';
import * as Production022 from '../components/ConsoleToggle';
import * as Production023 from '../components/CustomDialog';
import * as Production024 from '../components/DatabaseImport/HomebrewDatabaseDetector';
import * as Production025 from '../components/DatabaseImport/LegacyDatabaseImport';
import * as Production026 from '../components/DeviceSelection';
import * as Production027 from '../components/DiarizationModelManager';
import * as Production028 from '../components/EditableTitle';
import * as Production029 from '../components/EmbeddingModelSettings';
import * as Production030 from '../components/EmptyStateSummary';
import * as Production031 from '../components/GigaamModelManager';
import * as Production032 from '../components/GlobalRecordingPill';
import * as Production033 from '../components/GlobalSettingsButton';
import * as Production034 from '../components/ImportAudio/ImportAudioDialog';
import * as Production035 from '../components/ImportAudio/ImportDropOverlay';
import * as Production036 from '../components/Info';
import * as Production037 from '../components/KnowledgeReadinessCard';
import * as Production038 from '../components/LanguagePickerPopover';
import * as Production039 from '../components/LanguageSelection';
import * as Production040 from '../components/Logo';
import * as Production041 from '../components/MainContent/index';
import * as Production042 from '../components/MainNav/index';
import * as Production043 from '../components/ManagedDefaultsMigrationDialog';
import * as Production044 from '../components/MeetingConversation/AnalyticsReportButton';
import * as Production045 from '../components/MeetingConversation/AnalyticsReportDialog';
import * as Production046 from '../components/MeetingConversation/MeetingComposer';
import * as Production047 from '../components/MeetingConversation/MeetingConversation';
import * as Production048 from '../components/MeetingConversation/MeetingOverflowMenu';
import * as Production049 from '../components/MeetingConversation/SummaryMessage';
import * as Production050 from '../components/MeetingConversation/TranscriptCard';
import * as Production051 from '../components/MeetingConversation/TranscriptSearchDialog';
import * as Production052 from '../components/MeetingConversation/analytics/AnalyticsBuildPrompt';
import * as Production053 from '../components/MeetingConversation/analytics/MeetingDynamicsPanel';
import * as Production054 from '../components/MeetingConversation/analytics/MeetingNumbersPanel';
import * as Production055 from '../components/MeetingConversation/analytics/MeetingScoreSections';
import * as Production056 from '../components/MeetingConversation/analytics/MeetingTimelinePanel';
import * as Production057 from '../components/MeetingConversation/analytics/primitives';
import * as Production058 from '../components/MeetingDetails/DeleteMeetingButton';
import * as Production059 from '../components/MeetingDetails/DetectSpeakersButton';
import * as Production060 from '../components/MeetingDetails/InterviewWorkflowPanel';
import * as Production061 from '../components/MeetingDetails/LearningReviewPanel';
import * as Production062 from '../components/MeetingDetails/MeetingAudioPlayer';
import * as Production063 from '../components/MeetingDetails/MeetingContentWindowNotice';
import * as Production064 from '../components/MeetingDetails/OneOnOneWorkflowPanel';
import * as Production065 from '../components/MeetingDetails/RetranscribeDialog';
import * as Production066 from '../components/MeetingDetails/SpeakerNameCandidatesButton';
import * as Production067 from '../components/MeetingDetails/SpeakerRenameDialog';
import * as Production068 from '../components/MeetingDetails/StandupWorkflowPanel';
import * as Production069 from '../components/MeetingDetails/SummaryGeneratorButtonGroup';
import * as Production070 from '../components/MeetingDetails/SummaryPanel';
import * as Production071 from '../components/MeetingDetails/SummaryUpdaterButtonGroup';
import * as Production072 from '../components/MeetingDetails/TranscriptButtonGroup';
import * as Production073 from '../components/MeetingDetails/TranscriptPanel';
import * as Production074 from '../components/MeetingDetectionBanner';
import * as Production075 from '../components/MessageToast';
import * as Production076 from '../components/ModelDownloadProgress';
import * as Production077 from '../components/ModelSettingsModal';
import * as Production078 from '../components/ParakeetModelManager';
import * as Production079 from '../components/PermissionWarning';
import * as Production080 from '../components/PreferenceSettings';
import * as Production081 from '../components/PrivacySettings';
import * as Production082 from '../components/RecordingControls';
import * as Production083 from '../components/RecordingSettings';
import * as Production084 from '../components/RecordingStatusBar';
import * as Production085 from '../components/SettingTabs';
import * as Production086 from '../components/Sidebar/SidebarProvider';
import * as Production087 from '../components/SummaryLanguageSettings';
import * as Production088 from '../components/TranscriptRecovery/TranscriptRecovery';
import * as Production089 from '../components/TranscriptSettings';
import * as Production090 from '../components/TranscriptView';
import * as Production091 from '../components/UpcomingMeetings';
import * as Production092 from '../components/UpdateCheckProvider';
import * as Production093 from '../components/UpdateDialog';
import * as Production094 from '../components/UpdateNotification';
import * as Production095 from '../components/VirtualizedTranscriptView';
import * as Production096 from '../components/WhisperModelManager';
import * as Production097 from '../components/chat/ChatMarkdown';
import * as Production098 from '../components/chat/MessageBubble';
import * as Production099 from '../components/deslop-icons';
import * as Production100 from '../components/memento/Icon';
import * as Production101 from '../components/memento/RecordOverlay';
import * as Production102 from '../components/memento/Wordmark';
import * as Production103 from '../components/molecules/form-components/form-input-item';
import * as Production104 from '../components/molecules/form-components/form-input-switch';
import * as Production105 from '../components/molecules/form-components/form-select-item';
import * as Production106 from '../components/onboarding/OnboardingContainer';
import * as Production107 from '../components/onboarding/OnboardingFlow';
import * as Production108 from '../components/onboarding/OnboardingGate';
import * as Production109 from '../components/onboarding/shared/PermissionRow';
import * as Production110 from '../components/onboarding/shared/ProgressIndicator';
import * as Production111 from '../components/onboarding/shared/StatusIndicator';
import * as Production112 from '../components/onboarding/steps/ReadyStep';
import * as Production113 from '../components/onboarding/steps/PermissionsStep';
import * as Production114 from '../components/onboarding/steps/WelcomeStep';
import * as Production115 from '../components/shared/DownloadProgressToast';
import * as Production116 from '../components/ui/accordion';
import * as Production117 from '../components/ui/alert-dialog';
import * as Production118 from '../components/ui/alert';
import * as Production119 from '../components/ui/badge';
import * as Production120 from '../components/ui/bubble';
import * as Production121 from '../components/ui/button-group';
import * as Production122 from '../components/ui/button';
import * as Production123 from '../components/ui/card';
import * as Production124 from '../components/ui/checkbox';
import * as Production125 from '../components/ui/command';
import * as Production126 from '../components/ui/context-menu';
import * as Production127 from '../components/ui/dialog';
import * as Production128 from '../components/ui/drawer';
import * as Production129 from '../components/ui/dropdown-menu';
import * as Production130 from '../components/ui/dropdown';
import * as Production131 from '../components/ui/fluid-badge';
import * as Production132 from '../components/ui/fluid-button';
import * as Production133 from '../components/ui/fluid-dialog';
import * as Production134 from '../components/ui/fluid-input-group';
import * as Production135 from '../components/ui/fluid-input';
import * as Production136 from '../components/ui/fluid-radio-group';
import * as Production137 from '../components/ui/fluid-select';
import * as Production138 from '../components/ui/fluid-spinner';
import * as Production139 from '../components/ui/fluid-tabs';
import * as Production140 from '../components/ui/form';
import * as Production141 from '../components/ui/input-group';
import * as Production142 from '../components/ui/input';
import * as Production143 from '../components/ui/label';
import * as Production144 from '../components/ui/live-waveform';
import * as Production145 from '../components/ui/menu-item';
import * as Production146 from '../components/ui/message-scroller';
import * as Production147 from '../components/ui/message';
import * as Production148 from '../components/ui/popover';
import * as Production149 from '../components/ui/progress';
import * as Production150 from '../components/ui/prompt-input';
import * as Production151 from '../components/ui/radio-group';
import * as Production152 from '../components/ui/scroll-area';
import * as Production153 from '../components/ui/select';
import * as Production154 from '../components/ui/separator';
import * as Production155 from '../components/ui/sheet';
import * as Production156 from '../components/ui/sidebar';
import * as Production157 from '../components/ui/siri-wave-4';
import * as Production158 from '../components/ui/skeleton';
import * as Production159 from '../components/ui/sonner';
import * as Production160 from '../components/ui/switch';
import * as Production161 from '../components/ui/table';
import * as Production162 from '../components/ui/tabs';
import * as Production163 from '../components/ui/textarea';
import * as Production164 from '../components/ui/tooltip';
import * as Production165 from '../components/ui/visually-hidden';
import * as Production166 from '../vendor/deslop/mini-app/components/Markdown/index';
import * as Production167 from '../vendor/deslop/mini-app/components/Skeleton/index';
import * as Production168 from '../vendor/deslop/mini-app/components/Table/index';
import * as Production169 from '../vendor/deslop/mini-app/components/Text/index';

export const productionComponentModules = {
  'aisummary-block': Production000,
  'aisummary-block-note-summary-view': Production001,
  'aisummary-section': Production002,
  'aisummary': Production003,
  'about': Production004,
  'analytics-consent-switch': Production005,
  'analytics-data-modal': Production006,
  'analytics-provider': Production007,
  'app-sidebar': Production008,
  'audio-backend-selector': Production009,
  'audio-level-meter': Production010,
  'auto-meeting-detection': Production011,
  'beta-settings': Production012,
  'block-note-editor-basic-block-note-test': Production013,
  'block-note-editor-editor': Production014,
  'bluetooth-playback-warning': Production015,
  'built-in-model-manager': Production016,
  'calendar-settings': Production017,
  'chunk-progress-display': Production018,
  'compliance-notification': Production019,
  'confidence-indicator': Production020,
  'confirmation-model-confirmation-modal': Production021,
  'console-toggle': Production022,
  'custom-dialog': Production023,
  'database-import-homebrew-database-detector': Production024,
  'database-import-legacy-database-import': Production025,
  'device-selection': Production026,
  'diarization-model-manager': Production027,
  'editable-title': Production028,
  'embedding-model-settings': Production029,
  'empty-state-summary': Production030,
  'gigaam-model-manager': Production031,
  'global-recording-pill': Production032,
  'global-settings-button': Production033,
  'import-audio-import-audio-dialog': Production034,
  'import-audio-import-drop-overlay': Production035,
  'info': Production036,
  'knowledge-readiness-card': Production037,
  'language-picker-popover': Production038,
  'language-selection': Production039,
  'logo': Production040,
  'main-content': Production041,
  'main-nav': Production042,
  'managed-defaults-migration-dialog': Production043,
  'meeting-conversation-analytics-report-button': Production044,
  'meeting-conversation-analytics-report-dialog': Production045,
  'meeting-conversation-meeting-composer': Production046,
  'meeting-conversation-meeting-conversation': Production047,
  'meeting-conversation-meeting-overflow-menu': Production048,
  'meeting-conversation-summary-message': Production049,
  'meeting-conversation-transcript-card': Production050,
  'meeting-conversation-transcript-search-dialog': Production051,
  'meeting-conversation-analytics-analytics-build-prompt': Production052,
  'meeting-conversation-analytics-meeting-dynamics-panel': Production053,
  'meeting-conversation-analytics-meeting-numbers-panel': Production054,
  'meeting-conversation-analytics-meeting-score-sections': Production055,
  'meeting-conversation-analytics-meeting-timeline-panel': Production056,
  'meeting-conversation-analytics-primitives': Production057,
  'meeting-details-delete-meeting-button': Production058,
  'meeting-details-detect-speakers-button': Production059,
  'meeting-details-interview-workflow-panel': Production060,
  'meeting-details-learning-review-panel': Production061,
  'meeting-details-meeting-audio-player': Production062,
  'meeting-details-meeting-content-window-notice': Production063,
  'meeting-details-one-on-one-workflow-panel': Production064,
  'meeting-details-retranscribe-dialog': Production065,
  'meeting-details-speaker-name-candidates-button': Production066,
  'meeting-details-speaker-rename-dialog': Production067,
  'meeting-details-standup-workflow-panel': Production068,
  'meeting-details-summary-generator-button-group': Production069,
  'meeting-details-summary-panel': Production070,
  'meeting-details-summary-updater-button-group': Production071,
  'meeting-details-transcript-button-group': Production072,
  'meeting-details-transcript-panel': Production073,
  'meeting-detection-banner': Production074,
  'message-toast': Production075,
  'model-download-progress': Production076,
  'model-settings-modal': Production077,
  'parakeet-model-manager': Production078,
  'permission-warning': Production079,
  'preference-settings': Production080,
  'privacy-settings': Production081,
  'recording-controls': Production082,
  'recording-settings': Production083,
  'recording-status-bar': Production084,
  'setting-tabs': Production085,
  'sidebar-sidebar-provider': Production086,
  'summary-language-settings': Production087,
  'transcript-recovery-transcript-recovery': Production088,
  'transcript-settings': Production089,
  'transcript-view': Production090,
  'upcoming-meetings': Production091,
  'update-check-provider': Production092,
  'update-dialog': Production093,
  'update-notification': Production094,
  'virtualized-transcript-view': Production095,
  'whisper-model-manager': Production096,
  'chat-chat-markdown': Production097,
  'chat-message-bubble': Production098,
  'deslop-icons': Production099,
  'memento-icon': Production100,
  'memento-record-overlay': Production101,
  'memento-wordmark': Production102,
  'molecules-form-components-form-input-item': Production103,
  'molecules-form-components-form-input-switch': Production104,
  'molecules-form-components-form-select-item': Production105,
  'onboarding-onboarding-container': Production106,
  'onboarding-onboarding-flow': Production107,
  'onboarding-onboarding-gate': Production108,
  'onboarding-shared-permission-row': Production109,
  'onboarding-shared-progress-indicator': Production110,
  'onboarding-shared-status-indicator': Production111,
  'onboarding-steps-ready-step': Production112,
  'onboarding-steps-permissions-step': Production113,
  'onboarding-steps-welcome-step': Production114,
  'shared-download-progress-toast': Production115,
  'ui-accordion': Production116,
  'ui-alert-dialog': Production117,
  'ui-alert': Production118,
  'ui-badge': Production119,
  'ui-bubble': Production120,
  'ui-button-group': Production121,
  'ui-button': Production122,
  'ui-card': Production123,
  'ui-checkbox': Production124,
  'ui-command': Production125,
  'ui-context-menu': Production126,
  'ui-dialog': Production127,
  'ui-drawer': Production128,
  'ui-dropdown-menu': Production129,
  'ui-dropdown': Production130,
  'ui-fluid-badge': Production131,
  'ui-fluid-button': Production132,
  'ui-fluid-dialog': Production133,
  'ui-fluid-input-group': Production134,
  'ui-fluid-input': Production135,
  'ui-fluid-radio-group': Production136,
  'ui-fluid-select': Production137,
  'ui-fluid-spinner': Production138,
  'ui-fluid-tabs': Production139,
  'ui-form': Production140,
  'ui-input-group': Production141,
  'ui-input': Production142,
  'ui-label': Production143,
  'ui-live-waveform': Production144,
  'ui-menu-item': Production145,
  'ui-message-scroller': Production146,
  'ui-message': Production147,
  'ui-popover': Production148,
  'ui-progress': Production149,
  'ui-prompt-input': Production150,
  'ui-radio-group': Production151,
  'ui-scroll-area': Production152,
  'ui-select': Production153,
  'ui-separator': Production154,
  'ui-sheet': Production155,
  'ui-sidebar': Production156,
  'ui-siri-wave-4': Production157,
  'ui-skeleton': Production158,
  'ui-sonner': Production159,
  'ui-switch': Production160,
  'ui-table': Production161,
  'ui-tabs': Production162,
  'ui-textarea': Production163,
  'ui-tooltip': Production164,
  'ui-visually-hidden': Production165,
  'src-vendor-deslop-mini-app-components-markdown': Production166,
  'src-vendor-deslop-mini-app-components-skeleton': Production167,
  'src-vendor-deslop-mini-app-components-table': Production168,
  'src-vendor-deslop-mini-app-components-text': Production169,
};
