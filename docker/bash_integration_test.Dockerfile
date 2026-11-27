FROM specific-bash-version

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
printf '%s\n' \
'alias ll="ls -alF"' \
'export HISTTIMEFORMAT="%F %T  "' \
'enable flyline 2>/dev/null || enable -f /libflyline.so flyline' \
>> /root/.bashrc

RUN /bin/bash --version

# Neovim's terminal (libvterm) for tests/nvim_terminal_prompt.py.
RUN apt-get update && apt-get install -y --no-install-recommends neovim \
  && rm -rf /var/lib/apt/lists/*

COPY --from=built-artifact /libflyline.so /
COPY tests/focus_report_handoff.py /
COPY tests/resize_no_delete_line.py /
COPY tests/idle_redraw_bytes.py /
COPY tests/nvim_terminal_prompt.py /

RUN script -q -c "/bin/bash -i -c \"flyline --version && echo 'SUCCESS: Test completed'\"" /dev/null
RUN python3 /focus_report_handoff.py --bash /bin/bash --flyline /libflyline.so
RUN python3 /resize_no_delete_line.py --bash /bin/bash --flyline /libflyline.so
RUN python3 /idle_redraw_bytes.py --bash /bin/bash --flyline /libflyline.so
RUN python3 /nvim_terminal_prompt.py --bash /bin/bash --flyline /libflyline.so
