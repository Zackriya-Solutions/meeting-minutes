/** @type {import('tailwindcss').Config} */
module.exports = {
    darkMode: ['class'],
    content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
  	extend: {
  		fontFamily: {
  			sans: [
  				'var(--font-source-sans-3)'
  			]
  		},
  		colors: {
  			background: 'hsl(var(--background))',
  			foreground: 'hsl(var(--foreground))',
  			border: 'hsl(var(--border))',
  			input: 'hsl(var(--input))',
  			ring: 'hsl(var(--ring))',
  			// Raised panel surface (dark equivalent of the old `bg-white` cards/sidebar)
  			surface: '#262624',
  			// Granola-style warm dark theme: the stock light-mode ramps are remapped so
  			// existing classes keep their *semantic* role on a dark canvas.
  			// Low shades (50-300) stay dark (fills/borders), high shades (400-900)
  			// become light (icons/text) - i.e. the ramp is inverted around its middle.
  			gray: {
  				'50': '#212120',
  				'100': '#2C2C29',
  				'200': '#35352F',
  				'300': '#41413A',
  				'400': '#8C8C82',
  				'500': '#A4A499',
  				'600': '#BDBDB1',
  				'700': '#D1D1C6',
  				'800': '#E1E1D7',
  				'900': '#EDEDE4',
  				'950': '#F7F7F1'
  			},
  			blue: {
  				'50': '#1E2A3E',
  				'100': '#233650',
  				'200': '#2C4468',
  				'300': '#3A62A8',
  				'400': '#6FA0FF',
  				'500': '#5B96FF',
  				'600': '#4285F4',
  				'700': '#366FD6',
  				'800': '#9DC2FF',
  				'900': '#BDD6FF',
  				'950': '#16233B'
  			},
  			red: {
  				'50': '#2E1D1B',
  				'100': '#3B2422',
  				'200': '#4E2C29',
  				'300': '#6B3A35',
  				'400': '#F27E75',
  				'500': '#EF5B50',
  				'600': '#E8564C',
  				'700': '#E76F67',
  				'800': '#F5AFA8',
  				'900': '#FAD1CC',
  				'950': '#2A1614'
  			},
  			green: {
  				'50': '#1C2A20',
  				'100': '#223429',
  				'200': '#2B4534',
  				'300': '#3A5C45',
  				'400': '#5CCB8B',
  				'500': '#43BA76',
  				'600': '#3FAE6E',
  				'700': '#7DD8A6',
  				'800': '#A5E6C2',
  				'900': '#C8F0DA',
  				'950': '#142017'
  			},
  			yellow: {
  				'50': '#2C2718',
  				'100': '#383120',
  				'200': '#4A4128',
  				'300': '#635732',
  				'400': '#E8CA5B',
  				'500': '#DBB94A',
  				'600': '#CBA83E',
  				'700': '#EBD48A',
  				'800': '#F1E0A8',
  				'900': '#F7ECC7',
  				'950': '#211D10'
  			},
  			amber: {
  				'50': '#2C2515',
  				'100': '#3A301C',
  				'200': '#4C3F24',
  				'300': '#66532E',
  				'400': '#ECC153',
  				'500': '#DFAF45',
  				'600': '#D19E38',
  				'700': '#EDCF85',
  				'800': '#F3DFA6',
  				'900': '#F8EBC6',
  				'950': '#211B0E'
  			},
  			orange: {
  				'50': '#2E2113',
  				'100': '#3C2B19',
  				'200': '#4E3820',
  				'300': '#6B4B29',
  				'400': '#F49B52',
  				'500': '#ED8936',
  				'600': '#E07B2F',
  				'700': '#C9682C',
  				'800': '#F5C08E',
  				'900': '#F9D9B6',
  				'950': '#23180D'
  			},
  			purple: {
  				'50': '#261F33',
  				'100': '#2F2640',
  				'200': '#3C3054',
  				'300': '#4E3E70',
  				'400': '#B29BFF',
  				'500': '#A585FF',
  				'600': '#9A6BFF',
  				'700': '#C4AEFF',
  				'800': '#D6C7FF',
  				'900': '#E5DCFF',
  				'950': '#1D1730'
  			},
  			primary: {
  				DEFAULT: 'hsl(var(--primary))',
  				foreground: 'hsl(var(--primary-foreground))'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary))',
  				foreground: 'hsl(var(--secondary-foreground))'
  			},
  			tertiary: '#64748b',
  			card: {
  				DEFAULT: 'hsl(var(--card))',
  				foreground: 'hsl(var(--card-foreground))'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover))',
  				foreground: 'hsl(var(--popover-foreground))'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted))',
  				foreground: 'hsl(var(--muted-foreground))'
  			},
  			accent: {
  				DEFAULT: 'hsl(var(--accent))',
  				foreground: 'hsl(var(--accent-foreground))'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive))',
  				foreground: 'hsl(var(--destructive-foreground))'
  			},
  			chart: {
  				'1': 'hsl(var(--chart-1))',
  				'2': 'hsl(var(--chart-2))',
  				'3': 'hsl(var(--chart-3))',
  				'4': 'hsl(var(--chart-4))',
  				'5': 'hsl(var(--chart-5))'
  			}
  		},
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		},
  		keyframes: {
  			'accordion-down': {
  				from: {
  					height: '0'
  				},
  				to: {
  					height: 'var(--radix-accordion-content-height)'
  				}
  			},
  			'accordion-up': {
  				from: {
  					height: 'var(--radix-accordion-content-height)'
  				},
  				to: {
  					height: '0'
  				}
  			}
  		},
  		animation: {
  			'accordion-down': 'accordion-down 0.2s ease-out',
  			'accordion-up': 'accordion-up 0.2s ease-out'
  		}
  	}
  },
  plugins: [require("tailwindcss-animate")],
}