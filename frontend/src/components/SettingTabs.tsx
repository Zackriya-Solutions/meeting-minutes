import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ModelConfig, ModelSettingsModal } from "./ModelSettingsModal"
import { TranscriptModelProps } from "./TranscriptSettings"
import { ServerSettings } from "./ServerSettings"
import { About } from "./About";

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
    transcriptModelConfig, 
    setTranscriptModelConfig, 
    onSaveTranscript, 
    setSaveSuccess,
    defaultTab = "modelSettings"
}: SettingTabsProps) {

    const handleTabChange = () => {
        setSaveSuccess(null); // Reset save success when tab changes
    };

    return (
        <Tabs defaultValue={defaultTab} className="w-full" onValueChange={handleTabChange}>
  <TabsList>
    <TabsTrigger value="modelSettings">Model Settings</TabsTrigger>
    {/* <TabsTrigger value="transcriptSettings">Transcript Settings</TabsTrigger> */}
    <TabsTrigger value="serverSettings">Server Settings</TabsTrigger>
    <TabsTrigger value="about">About</TabsTrigger>
  </TabsList>
  <TabsContent value="modelSettings">
    <ModelSettingsModal

modelConfig={modelConfig}
setModelConfig={setModelConfig}
onSave={onSave}
/>
  </TabsContent>
  {/* <TabsContent value="transcriptSettings">
    <TranscriptSettings
    transcriptModelConfig={transcriptModelConfig}
    setTranscriptModelConfig={setTranscriptModelConfig}
    onSave={onSaveTranscript}
  />
  </TabsContent> */}
  <TabsContent value="serverSettings">
    <ServerSettings 
    setSaveSuccess={setSaveSuccess}
    />
  </TabsContent>
  <TabsContent value="about">
    <About />
  </TabsContent>
</Tabs>
    )
}


