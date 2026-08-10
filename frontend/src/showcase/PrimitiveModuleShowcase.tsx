'use client';

import React, { useState, type ComponentType, type ReactNode } from 'react';
import { useForm } from 'react-hook-form';
import Dropdown from '@/components/ui/dropdown';
import { MenuItem as ProductionMenuItem } from '@/components/ui/menu-item';

type ProductionModule = Record<string, unknown>;
type DynamicComponent = ComponentType<Record<string, unknown>>;

function get(module: ProductionModule, name: string): DynamicComponent {
  return module[name] as DynamicComponent;
}

function Surface({ children }: { children: ReactNode }) {
  return (
    <section className="rounded-[var(--ui-radius-20)] bg-[var(--elevation-1)] p-[var(--ui-space-24)]">
      {children}
    </section>
  );
}

function FormSpecimen({ module }: { module: ProductionModule }) {
  const form = useForm<{ meeting: string }>({ defaultValues: { meeting: 'Еженедельный синк' } });
  const Form = get(module, 'Form');
  const Field = get(module, 'FormField');
  const Item = get(module, 'FormItem');
  const Label = get(module, 'FormLabel');
  const Control = get(module, 'FormControl');
  const Description = get(module, 'FormDescription');
  return (
    <Form {...form}>
      <Field
        control={form.control}
        name="meeting"
        render={({ field }: { field: Record<string, unknown> }) => (
          <Item>
            <Label>Название встречи</Label>
            <Control><input className="h-9 w-full rounded-[20px] border border-[var(--primary-10)] bg-transparent px-3 text-[13px]" {...field} /></Control>
            <Description>Название хранится локально на устройстве.</Description>
          </Item>
        )}
      />
    </Form>
  );
}

export function PrimitiveModuleShowcase({ module, title }: { module: ProductionModule; title: string }) {
  const id = title.replace(/^ui[-/]/, '');
  const noop = () => undefined;
  const [open, setOpen] = useState(true);

  if (id === 'accordion') {
    const Root = get(module, 'Accordion');
    const Item = get(module, 'AccordionItem');
    const Trigger = get(module, 'AccordionTrigger');
    const Content = get(module, 'AccordionContent');
    return <Surface><Root type="single" defaultValue="local" collapsible><Item value="local"><Trigger>Что входит в локальную обработку?</Trigger><Content>Запись, расшифровка и подготовка итогов встречи.</Content></Item></Root></Surface>;
  }

  if (id === 'popover') {
    const Root = get(module, 'Popover');
    const Trigger = get(module, 'PopoverTrigger');
    const Content = get(module, 'PopoverContent');
    return <Surface><Root open={open} onOpenChange={setOpen}><Trigger className="rounded-md border border-border px-4 py-2">Открыть подсказку</Trigger><Content>Содержимое popover из production-компонента.</Content></Root></Surface>;
  }

  if (id === 'tooltip') {
    const Provider = get(module, 'TooltipProvider');
    const Root = get(module, 'Tooltip');
    const Trigger = get(module, 'TooltipTrigger');
    const Content = get(module, 'TooltipContent');
    return <Surface><Provider><Root open={open} onOpenChange={setOpen}><Trigger className="rounded-md border border-border px-4 py-2">Наведи курсор</Trigger><Content>Подсказка</Content></Root></Provider></Surface>;
  }

  if (id === 'dialog' || id === 'fluid-dialog') {
    const Root = get(module, 'Dialog');
    const Trigger = get(module, 'DialogTrigger');
    const Content = get(module, 'DialogContent');
    const Header = get(module, 'DialogHeader');
    const Title = get(module, 'DialogTitle');
    const Description = get(module, 'DialogDescription');
    const Close = get(module, 'DialogClose');
    return <Surface><Root open={open} onOpenChange={setOpen}><Trigger>Открыть диалог</Trigger><Content><Header><Title>Новая встреча</Title><Description>Проверь настройки перед началом записи.</Description></Header>{Close ? <Close>Закрыть</Close> : null}</Content></Root></Surface>;
  }

  if (id === 'alert-dialog') {
    const Root = get(module, 'AlertDialog');
    const Trigger = get(module, 'AlertDialogTrigger');
    const Content = get(module, 'AlertDialogContent');
    const Header = get(module, 'AlertDialogHeader');
    const Title = get(module, 'AlertDialogTitle');
    const Description = get(module, 'AlertDialogDescription');
    const Footer = get(module, 'AlertDialogFooter');
    const Cancel = get(module, 'AlertDialogCancel');
    const Action = get(module, 'AlertDialogAction');
    return <Surface><Root open={open} onOpenChange={setOpen}><Trigger>Удалить встречу</Trigger><Content><Header><Title>Удалить встречу?</Title><Description>Запись и расшифровка будут удалены с устройства.</Description></Header><Footer><Cancel>Отмена</Cancel><Action>Удалить</Action></Footer></Content></Root></Surface>;
  }

  if (id === 'sheet') {
    const Root = get(module, 'Sheet');
    const Trigger = get(module, 'SheetTrigger');
    const Content = get(module, 'SheetContent');
    const Header = get(module, 'SheetHeader');
    const Title = get(module, 'SheetTitle');
    const Description = get(module, 'SheetDescription');
    return <Surface><Root open={open} onOpenChange={setOpen}><Trigger>Открыть панель</Trigger><Content><Header><Title>Параметры записи</Title><Description>Выбери микрофон и источник системного звука.</Description></Header></Content></Root></Surface>;
  }

  if (id === 'drawer') {
    const Root = get(module, 'Drawer');
    const Trigger = get(module, 'DrawerTrigger');
    const Content = get(module, 'DrawerContent');
    const Header = get(module, 'DrawerHeader');
    const Title = get(module, 'DrawerTitle');
    const Description = get(module, 'DrawerDescription');
    return <Surface><Root open={open} onOpenChange={setOpen}><Trigger>Открыть drawer</Trigger><Content><Header><Title>Участники встречи</Title><Description>Переименуй спикеров после расшифровки.</Description></Header></Content></Root></Surface>;
  }

  if (id === 'dropdown-menu') {
    const Root = get(module, 'DropdownMenu');
    const Trigger = get(module, 'DropdownMenuTrigger');
    const Content = get(module, 'DropdownMenuContent');
    const Label = get(module, 'DropdownMenuLabel');
    const Item = get(module, 'DropdownMenuItem');
    const Separator = get(module, 'DropdownMenuSeparator');
    return <Surface><Root open={open} onOpenChange={setOpen}><Trigger className="rounded-md border border-border px-4 py-2">Действия</Trigger><Content><Label>Встреча</Label><Item>Переименовать</Item><Item>Экспортировать</Item><Separator /><Item>Удалить</Item></Content></Root></Surface>;
  }

  if (id === 'context-menu') {
    const Root = get(module, 'ContextMenu');
    const Trigger = get(module, 'ContextMenuTrigger');
    const Content = get(module, 'ContextMenuContent');
    const Item = get(module, 'ContextMenuItem');
    return <Surface><Root><Trigger className="block rounded-md border border-dashed border-border p-8">Нажми правой кнопкой</Trigger><Content><Item>Копировать</Item><Item>Экспортировать</Item></Content></Root></Surface>;
  }

  if (id === 'select' || id === 'fluid-select') {
    const Root = get(module, 'Select');
    const Trigger = get(module, 'SelectTrigger');
    const Value = module.SelectValue ? get(module, 'SelectValue') : null;
    const Content = get(module, 'SelectContent');
    const Item = get(module, 'SelectItem');
    return <Surface><Root defaultValue="gigaam" defaultOpen><Trigger>{Value ? <Value placeholder="Модель" /> : 'Модель'}</Trigger><Content><Item value="gigaam">GigaAM v3</Item><Item value="whisper">Whisper</Item></Content></Root></Surface>;
  }

  if (id === 'tabs' || id === 'fluid-tabs') {
    const Root = module.Tabs ? get(module, 'Tabs') : get(module, 'FluidTabs');
    const List = module.TabsList ? get(module, 'TabsList') : get(module, 'FluidTabsList');
    const Trigger = module.TabsTrigger ? get(module, 'TabsTrigger') : get(module, 'FluidTabsTrigger');
    const Content = module.TabsContent ? get(module, 'TabsContent') : get(module, 'FluidTabsContent');
    return <Surface><Root defaultValue="summary"><List><Trigger value="summary">Итоги</Trigger><Trigger value="transcript">Расшифровка</Trigger></List><Content value="summary">Краткие итоги встречи.</Content><Content value="transcript">Полная расшифровка разговора.</Content></Root></Surface>;
  }

  if (id === 'radio-group' || id === 'fluid-radio-group') {
    const Root = get(module, 'RadioGroup');
    const Item = module.RadioGroupItem ? get(module, 'RadioGroupItem') : get(module, 'RadioItem');
    return <Surface><Root defaultValue="local"><label className="flex items-center gap-2"><Item value="local" />Локальная модель</label><label className="mt-3 flex items-center gap-2"><Item value="cloud" />Облачная модель</label></Root></Surface>;
  }

  if (id === 'command') {
    const Root = get(module, 'Command');
    const Input = get(module, 'CommandInput');
    const List = get(module, 'CommandList');
    const Empty = get(module, 'CommandEmpty');
    const Group = get(module, 'CommandGroup');
    const Item = get(module, 'CommandItem');
    return <Surface><Root><Input placeholder="Найти действие…" /><List><Empty>Ничего не найдено</Empty><Group heading="Действия"><Item>Начать запись</Item><Item>Открыть календарь</Item><Item>Настройки</Item></Group></List></Root></Surface>;
  }

  if (id === 'dropdown' || id === 'menu-item') {
    return <Surface><Dropdown checkedIndex={0}><ProductionMenuItem index={0} label="Открыть встречу" checked /><ProductionMenuItem index={1} label="Экспортировать" /><ProductionMenuItem index={2} label="Удалить" /></Dropdown></Surface>;
  }

  if (id === 'form') {
    return <Surface><FormSpecimen module={module} /></Surface>;
  }

  if (id === 'card') {
    const Card = get(module, 'Card');
    const Header = get(module, 'CardHeader');
    const Title = get(module, 'CardTitle');
    const Description = get(module, 'CardDescription');
    const Content = get(module, 'CardContent');
    const Footer = get(module, 'CardFooter');
    return <Card><Header><Title>Командный синк</Title><Description>Сегодня, 11:00 · 32 минуты</Description></Header><Content>Подготовлены итоги и три задачи.</Content><Footer>Расшифровка сохранена локально</Footer></Card>;
  }

  if (id === 'alert') {
    const Alert = get(module, 'Alert');
    const Title = get(module, 'AlertTitle');
    const Description = get(module, 'AlertDescription');
    return <Alert><Title>Нужен доступ к микрофону</Title><Description>Разреши доступ в системных настройках, чтобы начать запись.</Description></Alert>;
  }

  if (id === 'table') {
    const Table = get(module, 'Table');
    const Header = get(module, 'TableHeader');
    const Body = get(module, 'TableBody');
    const Row = get(module, 'TableRow');
    const Head = get(module, 'TableHead');
    const Cell = get(module, 'TableCell');
    return <Table><Header><Row><Head>Участник</Head><Head>Вовлечённость</Head></Row></Header><Body><Row><Cell>Анна</Cell><Cell>58%</Cell></Row><Row><Cell>Игорь</Cell><Cell>42%</Cell></Row></Body></Table>;
  }

  if (id === 'input-group' || id === 'fluid-input-group') {
    const Group = get(module, 'InputGroup');
    const Input = module.InputGroupInput ? get(module, 'InputGroupInput') : get(module, 'InputField');
    const Addon = module.InputGroupAddon ? get(module, 'InputGroupAddon') : null;
    return <Surface><Group>{Addon ? <Addon>Тема</Addon> : null}<Input defaultValue="Еженедельный синк" /></Group></Surface>;
  }

  if (id === 'button-group') {
    const Group = get(module, 'ButtonGroup');
    const Text = get(module, 'ButtonGroupText');
    const Separator = get(module, 'ButtonGroupSeparator');
    return <Surface><Group><Text>Запись</Text><Separator /><Text>Расшифровка</Text><Separator /><Text>Итоги</Text></Group></Surface>;
  }

  if (id === 'progress') {
    const Root = get(module, 'Progress');
    return <Surface><Root value={64} aria-label="Загрузка модели: 64%" /></Surface>;
  }

  if (id === 'message-scroller') {
    const Provider = get(module, 'MessageScrollerProvider');
    const Root = get(module, 'MessageScroller');
    const Viewport = get(module, 'MessageScrollerViewport');
    const Content = get(module, 'MessageScrollerContent');
    const Item = get(module, 'MessageScrollerItem');
    return <Surface><div className="relative h-64"><Provider><Root><Viewport><Content>{Array.from({ length: 6 }, (_, index) => <Item key={index} data-message-id={`message-${index}`}>Фрагмент расшифровки {index + 1}: рабочая production-композиция.</Item>)}</Content></Viewport></Root></Provider></div></Surface>;
  }

  if (id === 'sidebar') {
    const Provider = get(module, 'SidebarProvider');
    const Sidebar = get(module, 'Sidebar');
    const Header = get(module, 'SidebarHeader');
    const Content = get(module, 'SidebarContent');
    const Group = get(module, 'SidebarGroup');
    const GroupLabel = get(module, 'SidebarGroupLabel');
    const GroupContent = get(module, 'SidebarGroupContent');
    const Menu = get(module, 'SidebarMenu');
    const MenuItem = get(module, 'SidebarMenuItem');
    const MenuButton = get(module, 'SidebarMenuButton');
    const Inset = get(module, 'SidebarInset');
    return <div className="h-[420px] overflow-hidden rounded-[var(--ui-radius-20)] border border-border"><Provider defaultOpen><Sidebar collapsible="none"><Header>Memento</Header><Content><Group><GroupLabel>Встречи</GroupLabel><GroupContent><Menu><MenuItem><MenuButton isActive>Сегодняшняя встреча</MenuButton></MenuItem><MenuItem><MenuButton>Командный синк</MenuButton></MenuItem><MenuItem><MenuButton>Интервью</MenuButton></MenuItem></Menu></GroupContent></Group></Content></Sidebar><Inset className="p-6">Содержимое выбранной встречи</Inset></Provider></div>;
  }

  if (id === 'scroll-area') {
    const Root = get(module, 'ScrollArea');
    const Bar = get(module, 'ScrollBar');
    return <Surface><Root className="h-40"><div className="space-y-3 pr-4">{Array.from({ length: 8 }, (_, index) => <p key={index}>Фрагмент расшифровки {index + 1}</p>)}</div><Bar /></Root></Surface>;
  }

  if (id === 'badge' || id === 'fluid-badge' || id === 'fluid-button') {
    const Primary = get(module, id === 'fluid-button' ? 'Button' : 'Badge');
    return <Surface><Primary>Готово</Primary></Surface>;
  }

  if (id === 'checkbox' || id === 'switch') {
    const Primary = get(module, id === 'checkbox' ? 'Checkbox' : 'Switch');
    return <Surface><label className="flex items-center gap-3"><Primary defaultChecked />Автоматически определять встречи</label></Surface>;
  }

  if (id === 'input' || id === 'fluid-input') {
    const Primary = get(module, 'Input');
    return <Surface><Primary defaultValue="Еженедельный синк" aria-label="Название встречи" /></Surface>;
  }

  if (id === 'textarea') {
    const Primary = get(module, 'Textarea');
    return <Surface><Primary defaultValue="Добавить контекст для итогов встречи" aria-label="Контекст встречи" /></Surface>;
  }

  if (id === 'label') {
    const Primary = get(module, 'Label');
    return <Surface><Primary>Название встречи</Primary></Surface>;
  }

  if (id === 'separator') {
    const Primary = get(module, 'Separator');
    return <Surface><div>До разделителя</div><Primary className="my-4" /><div>После разделителя</div></Surface>;
  }

  if (id === 'skeleton') {
    const Primary = get(module, 'Skeleton');
    return <Surface><Primary className="h-20 w-full" /></Surface>;
  }

  if (id === 'fluid-spinner') {
    const Primary = get(module, 'FluidSpinner');
    return <Surface><Primary className="h-8 w-8" /></Surface>;
  }

  if (id === 'live-waveform') {
    const Primary = get(module, 'LiveWaveform');
    return <Surface><Primary active className="h-20 w-full" /></Surface>;
  }

  if (id === 'visually-hidden') {
    const Primary = get(module, 'VisuallyHidden');
    return <Surface><span aria-hidden="true">Иконка записи</span><Primary>Начать запись</Primary></Surface>;
  }

  const preferredNames: Record<string, string> = {
    'siri-wave-4': 'SiriWave4',
  };
  const preferred = preferredNames[id];
  if (preferred && module[preferred]) {
    const Primary = get(module, preferred);
    return <Surface><Primary defaultValue="Пример" checked value="example" label="Пример" onCheckedChange={noop}>Пример</Primary></Surface>;
  }

  const first = Object.entries(module).find(([name, value]) => /^[A-Z]/.test(name) && (typeof value === 'function' || (typeof value === 'object' && value !== null && '$$typeof' in value)));
  if (!first) return <Surface>В модуле нет самостоятельного визуального компонента.</Surface>;
  const Primary = first[1] as DynamicComponent;
  return <Surface><Primary title={first[0]} label={first[0]} value="example" defaultValue="example" open checked onClick={noop}>Пример</Primary></Surface>;
}
