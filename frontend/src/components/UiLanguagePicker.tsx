"use client";

import { useState } from "react";
import { Check, ChevronsUpDown, Languages } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { useLocale } from "@/hooks/useLocale";
import { LOCALES, type Locale } from "@/i18n/config";
import { useTranslations } from "next-intl";

function localeKey(l: string): string {
  return l.toLowerCase().replace("-", "_");
}

export function UiLanguagePicker() {
  const t = useTranslations("settings");
  const { locale, setLocale } = useLocale();
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);

  const handleSelect = async (next: Locale) => {
    if (next === locale) {
      setOpen(false);
      return;
    }
    setPending(true);
    try {
      await setLocale(next);
      setOpen(false);
    } catch (e) {
      toast.error(t("preference.locale_change_failed"), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setPending(false);
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className="w-[220px] justify-between"
          disabled={pending}
        >
          <span className="flex items-center gap-2">
            <Languages className="h-4 w-4" />
            {t(`preference.locale_name_${localeKey(locale)}`)}
          </span>
          <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[240px] p-0" align="start">
        <Command>
          <CommandInput placeholder={t("preference.locale_search")} />
          <CommandList>
            <CommandEmpty>{t("preference.locale_empty")}</CommandEmpty>
            <CommandGroup>
              {LOCALES.map((l) => (
                <CommandItem
                  key={l}
                  value={l}
                  disabled={pending}
                  onSelect={() => handleSelect(l as Locale)}
                >
                  {t(`preference.locale_name_${localeKey(l)}`)}
                  <Check className={`ml-auto h-4 w-4 ${l === locale ? "opacity-100" : "opacity-0"}`} />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
