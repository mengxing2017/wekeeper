import DefaultTheme from "vitepress/theme";
import { h } from "vue";

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      "nav-bar-content-after": () =>
        h(
          "a",
          {
            href: "/admin",
            style: "margin-left:16px;font-size:14px;white-space:nowrap",
          },
          "返回工作台",
        ),
    }),
};
