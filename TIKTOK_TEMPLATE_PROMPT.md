# Prompt for AI Agent: Create TikTok-Style Subtitle Template

Please act as a DaVinci Resolve Fusion expert and help me create a custom `.drb` (DaVinci Resolve Bin) subtitle template for the "Auto-Subs" plugin. I want this template to look like the popular "TikTok-style" subtitles, where each word is highlighted sequentially as the speaker says it.

**Context & Requirements:**
- The Auto-Subs macro expects a specific structure. It injects an array of word timings into the Fusion Comp using `comp:FindTool("AutoSubs"):SetData("WordTiming", wordTiming)`.
- Each element in the `wordTiming` array has:
  - `startIndex`: The character start index of the word.
  - `endIndex`: The character end index of the word.
  - `startFrame`: The frame the word starts (relative to the clip).
  - `endFrame`: The frame the word ends (relative to the clip).
- The template must contain a `Text+` node (or similar) named exactly `Template`.
- The template needs an Expression or a modifier on the `Text+` node that reads the `WordTiming` data from the `AutoSubs` tool and dynamically changes the character level styling (e.g., color, size, or outline) for the word spanning from `startIndex` to `endIndex` during the `startFrame` to `endFrame` duration.

**Task:**
1. Provide the exact Fusion Node graph code (the `.setting` or copy-paste text representation) that includes the `AutoSubs` tool (with an empty `WordTiming` data block) and the `Template` text node.
2. Write the Lua Expression (or Character Level Styling modifier script) that parses the `WordTiming` array and applies the "TikTok" highlighting effect (e.g., bright yellow text, slight pop-in animation) to the active word based on the current time (`time`).
3. Explain how to wrap this into a `.drb` file and import it into DaVinci Resolve so the `autosubs_core.lua` script can automatically detect and use it.
