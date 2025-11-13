import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ModelConfig, ModelSettingsModal } from "./ModelSettingsModal"
import { TranscriptModelProps, TranscriptSettings } from "./TranscriptSettings"
import { RecordingSettings, RecordingPreferences } from "./RecordingSettings"
import { About } from "./About"
import { useTranslation } from 'react-i18next'

interface SettingTabsProps {
    modelConfig: ModelConfig;
    setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
    onSave: (config: ModelConfig) => void;
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onSaveTranscript: (config: TranscriptModelProps) => void;
    setSaveSuccess: (success: boolean | null) => void;
    defaultTab?: string;
}

export function SettingTabs({
    modelConfig,
    setModelConfig,
    onSave,
    setSaveSuccess,
    defaultTab = "transcriptSettings",
    transcriptModelConfig,
    setTranscriptModelConfig,
    onSaveTranscript,
}: SettingTabsProps) {
    const { t } = useTranslation('common')

    const handleTabChange = () => {
        setSaveSuccess(null); // Reset save success when tab changes
    };

    return (
        <Tabs defaultValue={defaultTab} className="w-full max-h-[calc(100vh-10rem)] overflow-y-auto" onValueChange={handleTabChange}>
  <TabsList>
    <TabsTrigger value="transcriptSettings">{t('settings.transcription.title')}</TabsTrigger>
    <TabsTrigger value="modelSettings">{t('settings.llm.title')}</TabsTrigger>
    <TabsTrigger value="recordingSettings">{t('settings.general')}</TabsTrigger>
    <TabsTrigger value="about">{t('settings.about')}</TabsTrigger>
  </TabsList>
  <TabsContent value="modelSettings">
    <ModelSettingsModal

modelConfig={modelConfig}
setModelConfig={setModelConfig}
onSave={onSave}
/>
  </TabsContent>
<TabsContent value="transcriptSettings">
    <TranscriptSettings
    transcriptModelConfig={transcriptModelConfig}
    setTranscriptModelConfig={setTranscriptModelConfig}
    // onSave={onSaveTranscript}
  />
  </TabsContent>
  <TabsContent value="recordingSettings">
    <RecordingSettings />
  </TabsContent>
  <TabsContent value="about">
    <About />
  </TabsContent>
</Tabs>
    )
}


