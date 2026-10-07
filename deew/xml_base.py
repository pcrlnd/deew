xml_dd_ddp_base = '''<?xml version="1.0"?>
<job_config>
  <input>
    <audio>
      <wav version="1">
        <file_name>-</file_name>    <!-- string -->
        <timecode_frame_rate>not_indicated</timecode_frame_rate>    <!-- One of: not_indicated, 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60 -->
        <offset>auto</offset>    <!-- string -->
        <ffoa>auto</ffoa>    <!-- string -->
        <storage>
          <local>
            <path>-</path>    <!-- string -->
          </local>
        </storage>
      </wav>
    </audio>
  </input>
  <filter>
    <audio>
      <pcm_to_ddp version="3">
        <loudness>    <!-- measure_only must be used in DD mode. -->
          <measure_only>
            <metering_mode>1770-3</metering_mode>    <!-- One of: 1770-3, 1770-2, 1770-1, LeqA -->
            <dialogue_intelligence>true</dialogue_intelligence>    <!-- boolean: true or false -->
            <speech_threshold>20</speech_threshold>    <!-- integer: from 0 to 100 -->
          </measure_only>
        </loudness>
        <encoder_mode>-</encoder_mode>    <!-- fixed value -->
        <bitstream_mode>complete_main</bitstream_mode>    <!-- One of: complete_main, music_and_effects, visually_impaired, hearing_impaired, dialogue, commentary, emergency, voice_over -->
        <downmix_config>off</downmix_config>    <!-- One of: off, mono, stereo, 5.1 -->
        <data_rate>-</data_rate>    <!-- One of: 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 144, 160, 176, 192, 200, 208, 216, 224, 232, 240, 248, 256, 272, 288, 304, 320, 336, 352, 368, 384, 400, 448, 512, 576, 640, 704, 768, 832, 896, 960, 1008, 1024 -->
        <timecode_frame_rate>not_indicated</timecode_frame_rate>    <!-- One of: not_indicated, 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60 -->
        <start>00:00:00.0</start>    <!-- string -->
        <end>end_of_file</end>    <!-- string -->
        <time_base>file_position</time_base>    <!-- One of: file_position, embedded_timecode -->
        <prepend_silence_duration>0.0</prepend_silence_duration>    <!-- string -->
        <append_silence_duration>0.0</append_silence_duration>    <!-- string -->
        <lfe_on>true</lfe_on>    <!-- boolean: true or false -->
        <dolby_surround_mode>not_indicated</dolby_surround_mode>    <!-- One of: yes, no, not_indicated -->
        <dolby_surround_ex_mode>no</dolby_surround_ex_mode>    <!-- One of: yes, no, not_indicated -->
        <user_data>-1</user_data>    <!-- integer: from -1 to 65535 -->
        <drc>
          <line_mode_drc_profile>-</line_mode_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech -->
          <rf_mode_drc_profile>-</rf_mode_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech -->
        </drc>
		<custom_dialnorm>-</custom_dialnorm>
        <lfe_lowpass_filter>true</lfe_lowpass_filter>    <!-- boolean: true or false -->
        <surround_90_degree_phase_shift>false</surround_90_degree_phase_shift>    <!-- boolean: true or false -->
        <surround_3db_attenuation>false</surround_3db_attenuation>    <!-- One of: true, false -->
        <downmix>
          <loro_center_mix_level>-3</loro_center_mix_level>    <!-- One of: +3, +1.5, 0, -1.5, -3, -4.5, -6, -inf -->
          <loro_surround_mix_level>-3</loro_surround_mix_level>    <!-- One of: -1.5, -3, -4.5, -6, -inf -->
          <ltrt_center_mix_level>-3</ltrt_center_mix_level>    <!-- One of: +3, +1.5, 0, -1.5, -3, -4.5, -6, -inf -->
          <ltrt_surround_mix_level>-3</ltrt_surround_mix_level>    <!-- One of: -1.5, -3, -4.5, -6, -inf -->
          <preferred_downmix_mode>loro</preferred_downmix_mode>    <!-- One of: loro, ltrt, ltrt-pl2, not_indicated -->
        </downmix>
        <allow_hybrid_downmix>false</allow_hybrid_downmix>    <!-- boolean: true or false -->
        <embedded_timecodes>
          <starting_timecode>off</starting_timecode>    <!-- 'off' disables embedded timecodes, 'auto' will use the 'start' value -->
          <frame_rate>auto</frame_rate>    <!-- One of: auto, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60 -->
        </embedded_timecodes>
      </pcm_to_ddp>
    </audio>
  </filter>
  <output>
    <ec3 version="1">
      <file_name>-</file_name>    <!-- string -->
      <storage>
        <local>
          <path>-</path>    <!-- string -->
        </local>
      </storage>
    </ec3>
  </output>
  <misc>
    <temp_dir>
      <clean_temp>true</clean_temp>
      <path>-</path>
    </temp_dir>
  </misc>
</job_config>'''

xml_thd_base = '''<?xml version="1.0"?>
<job_config>
  <input>
    <audio>
      <wav version="1">
        <file_name>-</file_name>    <!-- string -->
        <timecode_frame_rate>not_indicated</timecode_frame_rate>    <!-- One of: not_indicated, 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60 -->
        <offset>auto</offset>    <!-- string -->
        <ffoa>auto</ffoa>    <!-- string -->
        <storage>
          <local>
            <path>-</path>    <!-- string -->
          </local>
        </storage>
      </wav>
    </audio>
  </input>
  <filter>
    <audio>
      <encode_to_dthd version="1">
        <loudness_measurement>
          <metering_mode>1770-3</metering_mode>    <!-- One of: 1770-4, 1770-3, 1770-2, 1770-1, LeqA -->
          <dialogue_intelligence>true</dialogue_intelligence>    <!-- boolean: true or false -->
          <speech_threshold>20</speech_threshold>    <!-- integer: from 0 to 100 -->
        </loudness_measurement>
        <timecode_frame_rate>not_indicated</timecode_frame_rate>    <!-- One of: not_indicated, 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60 -->
        <start>first_frame_of_action</start>    <!-- string -->
        <end>end_of_file</end>    <!-- string -->
        <time_base>file_position</time_base>    <!-- One of: file_position, embedded_timecode -->
        <prepend_silence_duration>0</prepend_silence_duration>    <!-- string -->
        <append_silence_duration>0</append_silence_duration>    <!-- string -->
        <atmos_presentation>
          <drc_profile>-</drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech -->
          <spatial_clusters>12</spatial_clusters>    <!-- One of: 12, 14, 16 -->
          <legacy_authoring_compatibility>true</legacy_authoring_compatibility>    <!-- boolean: true or false -->
        </atmos_presentation>
        <presentation_8ch>
          <drc_profile>-</drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech -->
          <surround_3db_attenuation>false</surround_3db_attenuation>    <!-- boolean: true or false -->
        </presentation_8ch>
        <presentation_6ch>
          <drc_profile>-</drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech -->
          <surround_3db_attenuation>false</surround_3db_attenuation>    <!-- boolean: true or false -->
        </presentation_6ch>
        <presentation_2ch>
          <drc_profile>-</drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech -->
          <drc_default_on>true</drc_default_on>    <!-- boolean: true or false -->
          <format>stereo</format>    <!-- One of: stereo, dolby_surround_encoded, dolby_headphone_encoded -->
        </presentation_2ch>
        <optimize_data_rate>false</optimize_data_rate>    <!-- boolean: true or false -->
        <embedded_timecodes>
          <starting_timecode>off</starting_timecode>    <!-- string -->
          <frame_rate>auto</frame_rate>    <!-- One of: auto, 23.976, 24, 25, 29.97, 30 -->
        </embedded_timecodes>
        <log_format>txt</log_format>    <!-- One of: txt, html -->
		<custom_dialnorm>-</custom_dialnorm>
      </encode_to_dthd>
    </audio>
  </filter>
  <output>
    <mlp version="1">
      <file_name>-</file_name>    <!-- string -->
      <storage>
        <local>
          <path>-</path>    <!-- string -->
        </local>
      </storage>
    </mlp>
  </output>
  <misc>
    <temp_dir>
      <clean_temp>true</clean_temp>
      <path>-</path>
    </temp_dir>
  </misc>
</job_config>'''

xml_ac4_base = '''<?xml version="1.0"?>
<job_config>
  <input>
    <audio>
      <wav version="1">
        <file_name>-</file_name>    <!-- string -->
        <timecode_frame_rate>not_indicated</timecode_frame_rate>    <!-- One of: not_indicated, 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60 -->
        <offset>auto</offset>    <!-- string -->
        <ffoa>auto</ffoa>    <!-- string -->
        <storage>
          <local>
            <path>-</path>    <!-- string -->
          </local>
        </storage>
      </wav>
    </audio>
  </input>
  <filter>
    <audio>
      <encode_to_ims_ac4 version="1">
        <timecode_frame_rate>not_indicated</timecode_frame_rate>    <!-- One of: not_indicated, 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60 -->
        <start>first_frame_of_action</start>    <!-- string -->
        <end>end_of_file</end>    <!-- string -->
        <time_base>file_position</time_base>    <!-- One of: file_position, embedded_timecode -->
        <prepend_silence_duration>0</prepend_silence_duration>    <!-- string -->
        <append_silence_duration>0</append_silence_duration>    <!-- string -->
        <loudness>
          <measure_only>
            <metering_mode>1770-4</metering_mode>    <!-- One of: 1770-4, 1770-3, 1770-2, 1770-1, LeqA -->
            <dialogue_intelligence>true</dialogue_intelligence>    <!-- boolean: true or false -->
            <speech_threshold>15</speech_threshold>    <!-- integer: from 0 to 100 -->
          </measure_only>
        </loudness>
        <data_rate>-</data_rate>    <!-- One of: 64, 72 (< 5.3.0), 96 (5.3.0+), 112 (< 5.3.0), 128 (5.3.0+), 144, 256, 320 -->
        <ac4_frame_rate>native</ac4_frame_rate>    <!-- One of: native, 23.976, 24, 25, 29.97 -->
        <ims_legacy_presentation>false</ims_legacy_presentation>    <!-- boolean: true or false -->
        <iframe_interval>0</iframe_interval>    <!-- integer: from 0 to 1000 -->
        <language>LANGUAGE</language>    <!-- For example "eng-US". Remove this parameter to leave language tag empty. -->
        <encoding_profile>ims</encoding_profile>    <!-- One of: ims, ims_music -->
        <drc>
          <ddp_drc_profile>-</ddp_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech, none -->
          <flat_panel_drc_profile>-</flat_panel_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech, none -->
          <home_theatre_drc_profile>-</home_theatre_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech, none -->
          <portable_hp_drc_profile>-</portable_hp_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech, none -->
          <portable_spkr_drc_profile>-</portable_spkr_drc_profile>    <!-- One of: film_standard, film_light, music_standard, music_light, speech, none -->
        </drc>
      </encode_to_ims_ac4>
    </audio>
  </filter>
  <output>
    <ac4 version="1">
      <file_name>-</file_name>    <!-- string -->
      <storage>
        <local>
          <path>-</path>    <!-- string -->
        </local>
      </storage>
    </ac4>
  </output>
  <misc>
    <temp_dir>
      <clean_temp>true</clean_temp>
      <path>-</path>
    </temp_dir>
  </misc>
</job_config>'''