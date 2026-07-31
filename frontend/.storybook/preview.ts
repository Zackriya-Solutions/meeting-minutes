import type { Preview } from '@storybook/nextjs';

import '../src/app/globals.css';
import '../src/vendor/deslop/deslop-primitives.css';
import './preview.css';

const preview: Preview = {
  parameters: {
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
  },
};

export default preview;
