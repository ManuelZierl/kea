# Direct split suggestions

The composer suggestion renderer must not route every hint to the general actions
menu. Accepting a standalone `---` split is a one-click, undoable editor operation.
Capture its target and exact plan when rendering, then revalidate before applying.
Other recommendations may still require the menu and a preview. Component and
graphical tests cover target identity, stale plans, focus and no terminal input.
