# Manual digital-text test pack

These are synthetic TXT, Markdown, digital-PDF, and DOCX fixtures for manually exercising Provenance in the Tauri app. They contain no real student data. They test ordinary digital-text import, exact and modified overlap evidence, reference-library portability, signed reporting, and the explicitly labeled self-check report. They do not test OCR, images, scanned PDFs, or model-backed extraction; those inputs are intentionally unsupported.

## 1. Start the app

From the repository root, run:

```sh
pnpm --filter ./apps/app tauri dev
```

Keep the terminal open and wait for the desktop app to finish launching.

## 2. Create a current-session overlap test

1. Open **Sessions** and create a session named `Manual overlap test`.
2. Add three students named `Ada Sample`, `Ben Sample`, and `Cara Sample`.
3. For Ada, use **Upload file** and select `manual-test-pack/current/student-ada.txt`.
4. For Ben, upload `manual-test-pack/current/student-ben.md`.
5. For Cara, upload `manual-test-pack/current/student-cara.txt`.
6. If the session has an assignment-prompt field, paste in `manual-test-pack/current/assignment-prompt.txt`.
7. Choose **Analyze session**.

Review the pairwise evidence and the per-student details:

- Ada and Ben intentionally share one long paragraph verbatim. Look for an **exact** match and inspect both source passages.
- They also have a separate similar-but-edited paragraph. Look for a **modified** match and inspect both passages. Detection is evidence for review, not a finding of misconduct.
- Cara's response is an unrelated control. It should not share a substantial passage with Ada or Ben.
- Confirm that the assignment prompt is treated as prompt/common material rather than student copying, if prompt exclusion is shown in the review.

The fixture is designed to exercise those cases; scores and displayed boundaries are produced by the app and are not a promise of perfect detection.

## 3. Test each supported file-import format

This separate session checks that the same selectable text survives pasted text, TXT, Markdown, digital PDF, and DOCX imports.

1. Create a session named `Manual format test` and add five students named `Paste Sample`, `Text Sample`, `Markdown Sample`, `PDF Sample`, and `Word Sample`.
2. For Paste Sample, choose **Paste text** and paste the title and paragraph from `manual-test-pack/formats/cross-format-passage.txt`.
3. For the other students, upload the matching `.txt`, `.md`, `.pdf`, and `.docx` files from `manual-test-pack/formats/`.
4. Analyze the session. All five inputs intentionally contain the same text, so each pair should show the same exact shared passage. Compare the highlighted text and confirm the imported content is readable and equivalent across all five input methods.
5. If an input is rejected, imported as blank, or produces different text, note the filename/method and the app's exact message. The PDF in this pack has a real text layer; it is not a scan or image.

## 4. Exercise local reference-library export/import

1. In the analyzed session, use **Archive this session** and name it `Manual Test Library`. At least two non-empty submissions and a saved analysis are required.
2. Open **Reference Libraries** and choose **Export .plagpack** for `Manual Test Library`. Keep track of where the downloaded file was saved.
3. To test importing rather than keeping two copies, delete `Manual Test Library` from the app and confirm the deletion. This only deletes the local library entry; it does not delete the original session or the downloaded pack.
4. Choose **Import archive** and select the `.plagpack` file you just exported. Confirm that the imported library appears and its submission count is populated when opened.

## 5. Exercise session locking and certified reports

Return to `Manual overlap test` and:

1. Choose **Lock session for certification** and complete the operating-system Keychain/credential-store prompt if macOS displays one. The lock is intentionally irreversible in the UI, so do this only after reviewing the evidence.
2. Confirm that the session now says **Session locked · inputs frozen** and no longer offers editing or re-analysis.
3. Under per-student overlap, choose **Generate certified report for Ada Sample**. Save both the PDF and the companion signed JSON when prompted.
4. Check that the PDF identifies a teacher-certified report and includes a QR code; the signed JSON is required for full offline verification.
5. In **Verify a signed report**, choose the original JSON and press **Verify signature**. Expect a successful verification message.
6. In a terminal, run the helper below with the downloaded JSON path (quote the path if it contains spaces):

   ```sh
   node manual-test-pack/tamper-certified-report.mjs "/path/to/the/downloaded-report.json"
   ```

   The helper creates a sibling file ending in `.tampered.json` and does not overwrite the original. Select that tampered file in the verifier. It should be rejected. Verify the untouched original once more to confirm it remains valid.

## 6. Exercise the non-certified self-check path

1. Create a separate session named `Manual self-check` and add one student named `Riley Sample`.
2. Upload `manual-test-pack/self-check/self-check-student.txt`.
3. Select the imported reference library (the library created in step 3) in the session's reference-library section. With one current submission, a comparison library must be selected before analysis is available.
4. Choose **Analyze session**. Confirm that the selected library is listed as the comparison corpus and that the copied passage is shown as historical evidence, separately from current-session pairwise scores.
5. Choose **Download Self Check PDF** for Riley. Confirm the PDF says **Not Teacher Certified**. Do not lock this session as part of this self-check test.
6. As a negative check, create another one-submission session without selecting a library. Analysis should remain unavailable and the app should ask you to choose a comparison source. This is an explicit constraint, not an automatic alternate path.

## What to report back

For each step, note whether it passed and include any exact error text. For overlap results, report whether Ada/Ben show exact and modified evidence, whether Cara stays unrelated, and whether the highlighted source passages make sense. Keep the original certified JSON unchanged so it can be verified again.
