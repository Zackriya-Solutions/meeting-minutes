import './globals.css'
import { Source_Sans_3 } from 'next/font/google'
import { NextIntlClientProvider } from 'next-intl'
import { getLocale, getMessages } from 'next-intl/server'
import { getStoredLocale } from '@/i18n/request'
import { LocaleProvider } from '@/hooks/useLocale'
import ClientRootLayout from './ClientRootLayout'
import { Toaster } from 'sonner'
import { DEFAULT_LOCALE, isSupportedLocale, type Locale } from '@/i18n/config'

const sourceSans3 = Source_Sans_3({
  subsets: ['latin'],
  weight: ['400', '500', '600', '700'],
  variable: '--font-source-sans-3',
})

export { metadata } from './metadata'

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  // next-intl server helpers + Tauri stored-locale fallback.
  // PR-12 will replace the getStoredLocale() placeholder with the real
  // `get_ui_language` Tauri command once the Rust side lands.
  const intlLocale = await getLocale()
  const stored = await getStoredLocale()
  const locale: Locale = isSupportedLocale(stored) ? stored : (isSupportedLocale(intlLocale) ? intlLocale : DEFAULT_LOCALE)
  const messages = await getMessages()

  return (
    <html lang={locale}>
      <body className={`${sourceSans3.variable} font-sans antialiased`}>
        <NextIntlClientProvider locale={locale} messages={messages}>
          <LocaleProvider initial={locale}>
            <ClientRootLayout>{children}</ClientRootLayout>
          </LocaleProvider>
        </NextIntlClientProvider>
        <Toaster position="bottom-center" richColors closeButton />
      </body>
    </html>
  )
}