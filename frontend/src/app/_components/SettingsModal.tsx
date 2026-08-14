import { ModelConfig } from "@/components/ModelSettingsModal";
import { PreferenceSettings } from "@/components/PreferenceSettings";
import { DeviceSelection } from "@/components/DeviceSelection";
import { LanguageSelection } from "@/components/LanguageSelection";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { toast } from "sonner";
import { useConfig } from "@/contexts/ConfigContext";
import { useRecordingState } from "@/contexts/RecordingStateContext";
import { useT } from "@/lib/i18n";

type modalType = "modelSettings" | "deviceSettings" | "languageSettings" | "errorAlert" | "chunkDropWarning";

/**
 * SettingsModals Component
 *
 * All settings modals consolidated into a single component.
 * Uses ConfigContext and RecordingStateContext internally - no prop drilling needed!
 */

interface SettingsModalsProps {
  modals: {
    modelSettings: boolean;
    deviceSettings: boolean;
    languageSettings: boolean;
    errorAlert: boolean;
    chunkDropWarning: boolean;
  };
  messages: {
    errorAlert: string;
    chunkDropWarning: string;
  };
  onClose: (name: modalType) => void;
}

export function SettingsModals({
  modals,
  messages,
  onClose,
}: SettingsModalsProps) {
  // Contexts
  const {
    modelConfig,
    setModelConfig,
    models,
    modelOptions,
    error,
    selectedDevices,
    setSelectedDevices,
    selectedLanguage,
    setSelectedLanguage,
    transcriptModelConfig,
  } = useConfig();

  const { isRecording } = useRecordingState();
  const t = useT();

  return <>
    {/* Legacy Settings Modal */}
    <Dialog open={modals.modelSettings} onOpenChange={(open) => !open && onClose('modelSettings')}>
        <DialogContent size="lg" className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col p-0">
          {/* Header */}
          <div className="flex justify-between items-center p-6 border-b">
            <DialogTitle>{t('Preferences')}</DialogTitle>
          </div>

          {/* Content - Scrollable */}
          <div className="flex-1 overflow-y-auto p-6 space-y-8">
            {/* General Настройки Section */}
            <PreferenceSettings />

            {/* Divider */}
            <div className="border-t pt-8">
              <h4 className="text-lg font-semibold text-foreground mb-4">{t('AI Model Configuration')}</h4>
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-muted-foreground mb-1">
                    {t('Summarization Model')}
                  </label>
                  <div className="flex space-x-2">
                    <select
                      className="px-3 py-2 text-sm bg-background border border-border rounded-md shadow-none focus:outline-none focus:ring-1 focus:ring-ring focus:border-primary/40"
                      value={modelConfig.provider}
                      onChange={(e) => {
                        const provider = e.target.value as ModelConfig['provider'];
                        setModelConfig({
                          ...modelConfig,
                          provider,
                          model: modelOptions[provider][0]
                        });
                      }}
                    >
                      <option value="builtin-ai">{t('Built-in AI')}</option>
                      <option value="claude">Claude</option>
                      <option value="groq">Groq</option>
                      <option value="ollama">Ollama</option>
                      <option value="openrouter">OpenRouter</option>
                      <option value="openai">OpenAI</option>
                    </select>

                    <select
                      className="flex-1 px-3 py-2 text-sm bg-background border border-border rounded-md shadow-none focus:outline-none focus:ring-1 focus:ring-ring focus:border-primary/40"
                      value={modelConfig.model}
                      onChange={(e) => setModelConfig((prev: ModelConfig) => ({ ...prev, model: e.target.value }))}
                    >
                      {modelOptions[modelConfig.provider].map((model: string) => (
                        <option key={model} value={model}>
                          {model}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>
                {modelConfig.provider === 'ollama' && (
                  <div>
                    <h4 className="text-lg font-bold mb-4">{t('Available Ollama Models')}</h4>
                    {error && (
                      <div className="bg-destructive/10 border border-destructive/40 text-destructive px-4 py-3 rounded mb-4">
                        {error}
                      </div>
                    )}
                    <div className="grid gap-4 max-h-[400px] overflow-y-auto pr-2">
                      {models.map((model) => (
                        <div
                          key={model.id}
                          className={`bg-background p-4 rounded-lg shadow-none cursor-pointer transition-colors ${modelConfig.model === model.name ? 'ring-2 ring-ring bg-primary/10' : 'hover:bg-background'
                            }`}
                          onClick={() => setModelConfig((prev: ModelConfig) => ({ ...prev, model: model.name }))}
                        >
                          <h3 className="font-bold">{model.name}</h3>
                          <p className="text-muted-foreground">{t('Size:')} {model.size}</p>
                          <p className="text-muted-foreground">{t('Modified:')} {model.modified}</p>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Footer */}
          <div className="border-t p-6 flex justify-end">
            <button
              onClick={() => onClose('modelSettings')}
              className="px-4 py-2 text-sm font-medium text-primary-foreground bg-primary rounded-md hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-offset-2 ring-ring"
            >
              {t('Done')}
            </button>
          </div>
        </DialogContent>
    </Dialog>

    {/* Device Settings Modal */}
    <Dialog open={modals.deviceSettings} onOpenChange={(open) => !open && onClose('deviceSettings')}>
        <DialogContent className="max-w-md">
          <div className="flex justify-between items-center mb-4">
            <DialogTitle>{t('Audio Device Settings')}</DialogTitle>
          </div>

          <DeviceSelection
            selectedDevices={selectedDevices}
            onDeviceChange={setSelectedDevices}
            disabled={isRecording}
          />

          <div className="mt-6 flex justify-end">
            <button
              onClick={() => {
                const micDevice = selectedDevices.micDevice || 'Default';
                const systemDevice = selectedDevices.systemDevice || 'Default';
                toast.success(t("Devices selected"), {
                  description: `${t('Microphone:')} ${micDevice}, ${t('System Audio:')} ${systemDevice}`
                });
                onClose('deviceSettings');
              }}
              className="px-4 py-2 text-sm font-medium text-primary-foreground bg-primary rounded-md hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-offset-2 ring-ring"
            >
              {t('Done')}
            </button>
          </div>
        </DialogContent>
    </Dialog>

    {/* Настройки языка Modal */}
    <Dialog open={modals.languageSettings} onOpenChange={(open) => !open && onClose('languageSettings')}>
        <DialogContent className="max-w-md">
          <div className="flex justify-between items-center mb-4">
            <DialogTitle>{t('Language Settings')}</DialogTitle>
          </div>

          <LanguageSelection
            selectedLanguage={selectedLanguage}
            onLanguageChange={setSelectedLanguage}
            disabled={isRecording}
            provider={transcriptModelConfig.provider}
          />

          <div className="mt-6 flex justify-end">
            <button
              onClick={() => onClose('languageSettings')}
              className="px-4 py-2 text-sm font-medium text-primary-foreground bg-primary rounded-md hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-offset-2 ring-ring"
            >
              {t('Done')}
            </button>
          </div>
        </DialogContent>
    </Dialog>

    {/* Error Alert Modal */}
    <Dialog open={modals.errorAlert} onOpenChange={(open) => !open && onClose('errorAlert')}>
      <DialogContent className="max-w-md p-0">
        <Alert className="max-w-md mx-4 border-destructive/40 bg-background shadow-none">
          <AlertTitle className="text-destructive">{t('Recording Stopped')}</AlertTitle>
          <AlertDescription className="text-destructive">
            {messages.errorAlert}
            <button
              onClick={() => onClose('errorAlert')}
              className="ml-2 text-destructive hover:opacity-80 underline"
            >
              {t('Dismiss')}
            </button>
          </AlertDescription>
        </Alert>
      </DialogContent>
    </Dialog>

    {/* Chunk Drop Warning Modal */}
    <Dialog open={modals.chunkDropWarning} onOpenChange={(open) => !open && onClose('chunkDropWarning')}>
      <DialogContent size="lg" className="max-w-lg p-0">
        <Alert className="max-w-lg mx-4 border-primary/40 bg-background shadow-none">
          <AlertTitle className="text-primary">{t('Transcription Performance Warning')}</AlertTitle>
          <AlertDescription className="text-primary">
            {messages.chunkDropWarning}
            <button
              onClick={() => onClose('chunkDropWarning')}
              className="ml-2 text-primary hover:text-primary/90 underline"
            >
              {t('Dismiss')}
            </button>
          </AlertDescription>
        </Alert>
      </DialogContent>
    </Dialog>
  </>
}
