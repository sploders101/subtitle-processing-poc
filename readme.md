# Subtitle Processing POC

This repo is a proof-of-concept for end-to-end processing of subtitles, preparing them for analysis.

This code will eventually make its way into Mediacorral's worker processes to allow for better progress
updates & workload clustering.

This code, in its current form will open `test.mkv`, find the first subtitle track it sees (this will be
more methodical after integrating into Mediacorral), and extract the subtitles. If the subtitles are in
SRT format, they are printed directly to the screen. If they are in VobSub format, they are rendered to
an image buffer, transformed to optimize for OCR, and sent to Tesseract to identify text. The rendered
image, processed image, and text are all printed to the console (images are printed using sixel encoding).
