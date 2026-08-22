import type { Preview } from "@storybook/react-vite";
import "../styles.css";

const preview: Preview = {
  parameters: {
    backgrounds: {
      default: "ghostex dark",
      values: [{ name: "ghostex dark", value: "#050505" }],
    },
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
    layout: "fullscreen",
  },
};

export default preview;
